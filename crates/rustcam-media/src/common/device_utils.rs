
pub fn stream_key_to_cache_key(stream_key: &str) -> String {
    stream_key.replace('_', "/")
}

pub fn cache_key_to_stream_key(cache_key: &str) -> String {
    cache_key.replace('/', "_")
}

pub fn format_cache_key_opt(device_tag: Option<&str>, channel_tag: Option<&str>) -> String {
    format!(
        "{}/{}",
        device_tag.unwrap_or_default(),
        channel_tag.unwrap_or_default()
    )
}
pub fn format_cache_key(device_tag:&str,channel_tag:&str) -> String{
  format!(
        "{}/{}",
        device_tag,
        channel_tag
    )
}
pub fn format_stream_key(device_tag: &str, channel_tag: &str) -> String {
    format!("{}_{}", device_tag, channel_tag)
}

pub fn parse_stream_key(stream_key: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = stream_key.split('_').collect();
    if parts.len() >= 2 {
        let channel_tag = parts.last().unwrap();
        let device_tag = &stream_key[..stream_key.len() - channel_tag.len() - 1];
        if !device_tag.is_empty() && !channel_tag.is_empty() {
            return Some((device_tag.to_string(), channel_tag.to_string()));
        }
    }
    None
}

pub fn parse_stream_key_loose(stream_key: &str) -> Option<(String, String)> {
    if let Some((device_tag, channel_tag)) = stream_key.split_once('/') {
        if !device_tag.is_empty() && !channel_tag.is_empty() {
            return Some((device_tag.to_string(), channel_tag.to_string()));
        }
    }
    parse_stream_key(stream_key)
}
