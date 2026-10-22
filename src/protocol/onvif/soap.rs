use roxmltree::Document;

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

pub fn local_name(name: &str) -> &str {
    if let Some(idx) = name.find(':') {
        &name[idx + 1..]
    } else {
        name
    }
}

pub fn extract_action(soap_body: &[u8]) -> Option<String> {
    let xml_str = String::from_utf8_lossy(soap_body).into_owned();
    let doc = Document::parse(&xml_str).ok()?;
    let root = doc.root_element();

    for node in root.children() {
        if node.is_element() {
            let name = node.tag_name().name();
            if name == "Body" {
                for child in node.children() {
                    if child.is_element() {
                        return Some(child.tag_name().name().to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn extract_element_text(soap_body: &[u8], element: &str) -> Option<String> {
    let xml_str = String::from_utf8_lossy(soap_body).into_owned();
    let doc = Document::parse(&xml_str).ok()?;
    let root = doc.root_element();

    fn find_element<'a, 'input: 'a>(
        node: roxmltree::Node<'a, 'input>,
        element: &str,
    ) -> Option<roxmltree::Node<'a, 'input>> {
        if node.is_element() && local_name(node.tag_name().name()) == element {
            return Some(node);
        }
        for child in node.children() {
            if let Some(found) = find_element(child, element) {
                return Some(found);
            }
        }
        None
    }

    let target = find_element(root, element)?;

    let mut result = String::new();
    for child in target.children() {
        if child.is_text() {
            result.push_str(child.text().unwrap_or(""));
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn extract_attribute(soap_body: &[u8], parent: &str, attr: &str) -> Option<String> {
    let xml_str = String::from_utf8_lossy(soap_body).into_owned();
    let doc = Document::parse(&xml_str).ok()?;
    let root = doc.root_element();

    fn find_element<'a, 'input: 'a>(
        node: roxmltree::Node<'a, 'input>,
        element: &str,
    ) -> Option<roxmltree::Node<'a, 'input>> {
        if node.is_element() && local_name(node.tag_name().name()) == element {
            return Some(node);
        }
        for child in node.children() {
            if let Some(found) = find_element(child, element) {
                return Some(found);
            }
        }
        None
    }

    let target = find_element(root, parent)?;
    for a in target.attributes() {
        if a.name() == attr {
            return Some(a.value().to_string());
        }
    }
    None
}

pub fn extract_services_xaddr(soap_body: &[u8]) -> Option<String> {
    let xml_str = String::from_utf8_lossy(soap_body).into_owned();
    let doc = Document::parse(&xml_str).ok()?;
    let root = doc.root_element();

    for node in root.descendants() {
        if node.is_element() && local_name(node.tag_name().name()) == "Service" {
            if let Some(ns) = node.attribute("Namespace") {
                if ns.contains("media") {
                    return node.attribute("XAddr").map(|s| s.to_string());
                }
            }
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
    fn test_extract_uri_with_ampersand() {
        let body = br#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:trt="http://www.onvif.org/ver10/media/wsdl">
<env:Body><trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>rtsp://192.168.1.211:554/Streaming/Channels/101?transportmode=unicast&amp;profile=Profile_1</tt:Uri>
<tt:InvalidAfterConnect>false</tt:InvalidAfterConnect>
<tt:Timeout>PT60S</tt:Timeout>
</trt:MediaUri>
</trt:GetStreamUriResponse>
</env:Body>
</env:Envelope>"#;

        let uri = extract_element_text(body, "Uri");

        assert!(uri.is_some(), "URI should be extracted");
        let uri_str = uri.unwrap();

        assert!(
            uri_str.contains("transportmode=unicast&profile=Profile_1"),
            "URI should contain '&' but got: {}",
            uri_str
        );
    }

    #[test]
    fn test_extract_simple_text() {
        let body = br#"<root><Name>mainStream</Name></root>"#;
        let name = extract_element_text(body, "Name");
        assert_eq!(name, Some("mainStream".to_string()));
    }

    #[test]
    fn test_local_name() {
        assert_eq!(local_name("tt:Uri"), "Uri");
        assert_eq!(local_name("Uri"), "Uri");
        assert_eq!(local_name("env:Body"), "Body");
    }
}
