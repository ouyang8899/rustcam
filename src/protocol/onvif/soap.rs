use quick_xml::events::Event;
use quick_xml::Reader;

pub fn build_soap_response(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body>{}</s:Body>
</s:Envelope>"#,
        body
    )
}

pub fn build_soap_fault(fault_code: &str, fault_subcode: &str, fault_reason: &str) -> String {
    build_soap_response(&format!(
        r#"<s:Fault>
  <s:Code><s:Value>{}</s:Value><s:Subcode><s:Value>{}</s:Value></s:Subcode></s:Code>
  <s:Reason><s:Text>{}</s:Text></s:Reason>
</s:Fault>"#,
        fault_code, fault_subcode, fault_reason
    ))
}

fn local_name(name_bytes: &[u8]) -> &str {
    let name_str = std::str::from_utf8(name_bytes).unwrap_or("");
    if let Some(idx) = name_str.find(':') {
        &name_str[idx + 1..]
    } else {
        name_str
    }
}

/// 从 SOAP 请求中提取操作名
pub fn extract_action(soap_body: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(soap_body);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = String::from_utf8_lossy(&name_bytes);
                if name.ends_with(":Envelope") || name == "Envelope"
                    || name.ends_with(":Body") || name == "Body"
                {
                    continue;
                }
                return Some(local_name(&name_bytes).to_string());
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

/// 提取指定元素的文本内容
pub fn extract_element_text(soap_body: &[u8], element: &str) -> Option<String> {
    let mut reader = Reader::from_reader(soap_body);
    reader.config_mut().trim_text(true);
    let mut capture = false;
    let mut result = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = local_name(&name_bytes);
                if name == element {
                    capture = true;
                }
            }
            Ok(Event::Text(ref e)) if capture => {
                result.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = local_name(&name_bytes);
                if name == element {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result.trim().to_string())
    }
}

/// 提取指定元素的属性值
pub fn extract_attribute(soap_body: &[u8], parent: &str, attr: &str) -> Option<String> {
    let mut reader = Reader::from_reader(soap_body);
    reader.config_mut().trim_text(true);
    let mut in_parent = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = local_name(&name_bytes);

                if name == parent {
                    in_parent = true;
                }

                if in_parent {
                    for a in e.attributes().flatten() {
                        let key_bytes = a.key.as_ref().to_vec();
                        let key = std::str::from_utf8(&key_bytes).unwrap_or("");
                        if key == attr {
                            let value = String::from_utf8_lossy(&a.value);
                            return Some(value.to_string());
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let name = local_name(&name_bytes);
                if name == parent {
                    in_parent = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_action() {
        let body = br#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body><GetDeviceInformation xmlns="http://www.onvif.org/ver10/device/wsdl"/></s:Body>
</s:Envelope>"#;
        assert_eq!(extract_action(body).unwrap(), "GetDeviceInformation");
    }

    #[test]
    fn test_extract_action_with_ns() {
        let body = br#"<?xml version="1.0"?>
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
<env:Body><ns:GetStreamUri xmlns:ns="http://www.onvif.org/ver10/media/wsdl"/></env:Body>
</env:Envelope>"#;
        assert_eq!(extract_action(body).unwrap(), "GetStreamUri");
    }

    #[test]
    fn test_extract_element_text() {
        let body = br#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body><Username>admin</Username></s:Body>
</s:Envelope>"#;
        assert_eq!(extract_element_text(body, "Username"), Some("admin".to_string()));
    }

    #[test]
    fn test_extract_element_text_ns() {
        let body = br#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope">
<s:Body><wsse:Username xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">admin</wsse:Username></s:Body>
</s:Envelope>"#;
        assert_eq!(extract_element_text(body, "Username"), Some("admin".to_string()));
    }

    #[test]
    fn test_build_soap_response() {
        let resp = build_soap_response("<GetDeviceInformationResponse><Manufacturer>RustCam</Manufacturer></GetDeviceInformationResponse>");
        assert!(resp.contains("s:Envelope"));
        assert!(resp.contains("RustCam"));
    }

    #[test]
    fn test_build_soap_fault() {
        let fault = build_soap_fault("env:Sender", "ter:NotAuthenticated", "Authentication failed");
        assert!(fault.contains("env:Sender"));
        assert!(fault.contains("ter:NotAuthenticated"));
        assert!(fault.contains("Authentication failed"));
    }
}