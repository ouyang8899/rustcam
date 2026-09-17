#[derive(Debug, Clone)]
pub struct RtspRequest {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RtspRequest {
    pub fn new(method: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            uri: uri.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn build(&self, cseq: u32) -> String {
        let mut s = format!("{} {} RTSP/1.0\r\n", self.method, self.uri);
        s.push_str(&format!("CSeq: {}\r\n", cseq));

        for (key, value) in &self.headers {
            s.push_str(&format!("{}: {}\r\n", key, value));
        }

        if let Some(body) = &self.body {
            s.push_str(&format!("Content-Length: {}\r\n", body.len()));
            s.push_str("\r\n");
            s.push_str(body);
        } else {
            s.push_str("\r\n");
        }

        s
    }
}

#[derive(Debug, Clone)]
pub struct RequestBuilder {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl RequestBuilder {
    pub fn new(method: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            uri: uri.into(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn build(&self, cseq: u32) -> String {
        let mut s = format!("{} {} RTSP/1.0\r\n", self.method, self.uri);
        s.push_str(&format!("CSeq: {}\r\n", cseq));

        for (key, value) in &self.headers {
            s.push_str(&format!("{}: {}\r\n", key, value));
        }

        if let Some(body) = &self.body {
            s.push_str(&format!("Content-Length: {}\r\n", body.len()));
            s.push_str("\r\n");
            s.push_str(body);
        } else {
            s.push_str("\r\n");
        }

        s
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }
}
