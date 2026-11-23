use quick_xml::events::Event;
use quick_xml::Reader;

use crate::protocol::onvif::soap_client::OnvifSoapClient;

const PTZ_WSDL: &str = "http://www.onvif.org/ver20/ptz/wsdl";

pub struct OnvifPtzClient {
    soap: OnvifSoapClient,

    profile_token: String,
}

impl OnvifPtzClient {
    pub fn new(ptz_url: String, username: Option<String>, password: Option<String>) -> Self {
        Self {
            soap: OnvifSoapClient::new(ptz_url, username, password),
            profile_token: "Profile_1".to_string(),
        }
    }

    pub fn from_host_port(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        Self::new(
            format!("http://{}:{}/onvif/ptz_service", host, port),
            username,
            password,
        )
    }

    pub fn with_profile(mut self, profile_token: String) -> Self {
        self.profile_token = profile_token;
        self
    }

    pub fn set_profile(&mut self, profile_token: String) {
        self.profile_token = profile_token;
    }

    pub fn profile_token(&self) -> &str {
        &self.profile_token
    }

    pub async fn continuous_move(&mut self, pan: f64, tilt: f64, zoom: f64) -> anyhow::Result<()> {
        let body = self.continuous_move_body(pan, tilt, zoom);
        self.send_action("ContinuousMove", &body).await.map(|_| ())
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        let body = format!(
            r#"<Stop xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<PanTilt>true</PanTilt>
<Zoom>true</Zoom>
</Stop>"#,
            self.profile_token
        );
        self.send_action("Stop", &body).await.map(|_| ())
    }

    pub async fn goto_preset(&mut self, preset_token: &str) -> anyhow::Result<()> {
        let body = format!(
            r#"<GotoPreset xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<PresetToken>{}</PresetToken>
</GotoPreset>"#,
            self.profile_token, preset_token
        );
        self.send_action("GotoPreset", &body).await.map(|_| ())
    }

    pub async fn set_preset(
        &mut self,
        preset_token: &str,
        preset_name: Option<&str>,
    ) -> anyhow::Result<String> {
        let name_xml = preset_name
            .map(|n| format!(r#"<PresetName>{}</PresetName>"#, n))
            .unwrap_or_default();
        let body = format!(
            r#"<SetPreset xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<PresetToken>{}</PresetToken>
{}
</SetPreset>"#,
            self.profile_token, preset_token, name_xml
        );
        let resp = self.send_action("SetPreset", &body).await?;
        self.parse_preset_token(&resp)
            .ok_or_else(|| anyhow::anyhow!("No preset token in response"))
    }

    pub async fn remove_preset(&mut self, preset_token: &str) -> anyhow::Result<()> {
        let body = format!(
            r#"<RemovePreset xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<PresetToken>{}</PresetToken>
</RemovePreset>"#,
            self.profile_token, preset_token
        );
        self.send_action("RemovePreset", &body).await.map(|_| ())
    }

    pub async fn absolute_move(&mut self, pan: f64, tilt: f64, zoom: f64) -> anyhow::Result<()> {
        let body = format!(
            r#"<AbsoluteMove xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<Position>
<PanTilt x="{}" y="{}" xmlns="http://www.onvif.org/ver10/schema"/>
<Zoom x="{}" xmlns="http://www.onvif.org/ver10/schema"/>
</Position>
</AbsoluteMove>"#,
            self.profile_token, pan, tilt, zoom
        );
        self.send_action("AbsoluteMove", &body).await.map(|_| ())
    }

    pub async fn relative_move(&mut self, pan: f64, tilt: f64, zoom: f64) -> anyhow::Result<()> {
        let body = format!(
            r#"<RelativeMove xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<Translation>
<PanTilt x="{}" y="{}" xmlns="http://www.onvif.org/ver10/schema"/>
<Zoom x="{}" xmlns="http://www.onvif.org/ver10/schema"/>
</Translation>
</RelativeMove>"#,
            self.profile_token, pan, tilt, zoom
        );
        self.send_action("RelativeMove", &body).await.map(|_| ())
    }

    pub async fn get_status(&mut self) -> anyhow::Result<crate::domain::ptz::PtzStatus> {
        let body = format!(
            r#"<GetStatus xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
</GetStatus>"#,
            self.profile_token
        );
        let resp = self.send_action("GetStatus", &body).await?;
        self.parse_ptz_status(&resp)
    }

    pub async fn get_presets(&mut self) -> anyhow::Result<Vec<(String, Option<String>)>> {
        let body = format!(
            r#"<GetPresets xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
</GetPresets>"#,
            self.profile_token
        );
        let resp = self.send_action("GetPresets", &body).await?;
        Ok(self.parse_presets(&resp))
    }

    fn continuous_move_body(&self, pan: f64, tilt: f64, zoom: f64) -> String {
        format!(
            r#"<ContinuousMove xmlns="http://www.onvif.org/ver20/ptz/wsdl">
<ProfileToken>{}</ProfileToken>
<Velocity>
<PanTilt x="{}" y="{}" xmlns="http://www.onvif.org/ver10/schema"/>
<Zoom x="{}" xmlns="http://www.onvif.org/ver10/schema"/>
</Velocity>
</ContinuousMove>"#,
            self.profile_token, pan, tilt, zoom
        )
    }

    async fn send_action(&mut self, action: &str, body: &str) -> anyhow::Result<String> {
        let full_action = format!("{}/{}", PTZ_WSDL, action);
        self.soap.post(Some(&full_action), body).await
    }

    fn parse_ptz_status(&self, body: &str) -> anyhow::Result<crate::domain::ptz::PtzStatus> {
        let mut reader = Reader::from_str(body);
        reader.config_mut().trim_text(true);

        let mut pan = None;
        let mut tilt = None;
        let mut zoom = None;
        let mut moving = false;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();

                    if name.into_inner() == "PanTilt" {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if let Ok(val) = attr.value.parse::<f64>() {
                                match key {
                                    "x" => pan = Some(val),
                                    "y" => tilt = Some(val),
                                    _ => {}
                                }
                            }
                        }
                    }
                    if name.into_inner() == "Zoom" {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "x" {
                                zoom = attr.value.parse::<f64>().ok();
                            }
                        }
                    }
                    if name.into_inner() == "MoveStatus" {
                        for attr in e.attributes().flatten() {
                            if attr.value.as_ref() != "IDLE" {
                                moving = true;
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }

        Ok(crate::domain::ptz::PtzStatus {
            position_pan: pan,
            position_tilt: tilt,
            position_zoom: zoom,
            moving,
        })
    }

    fn parse_preset_token(&self, body: &str) -> Option<String> {
        let mut reader = Reader::from_str(body);
        reader.config_mut().trim_text(true);

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();

                    if name.into_inner() == "Preset" || name.into_inner() == "PresetToken" {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "token" {
                                return Some(attr.value.to_string());
                            }
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name();
                    if name.into_inner() == "PresetToken" {}
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }
        None
    }

    fn parse_presets(&self, body: &str) -> Vec<(String, Option<String>)> {
        let mut results = Vec::new();
        let mut reader = Reader::from_str(body);
        reader.config_mut().trim_text(true);
        let mut current_token: Option<String> = None;
        let mut current_name: Option<String> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name().local_name();
                    if name.into_inner() == "Preset" || name.into_inner() == "PTZPreset" {
                        current_token = None;
                        current_name = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner();
                            if key == "token" {
                                current_token = Some(attr.value.to_string());
                            }
                        }
                    } else if name.into_inner() == "Name" {
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if current_token.is_some() && current_name.is_none() {
                        let text = (&*e).to_string();
                        if !text.is_empty() {
                            current_name = Some(text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let name = e.name().local_name();
                    if (name.into_inner() == "Preset" || name.into_inner() == "PTZPreset")
                        && current_token.is_some()
                    {
                        if let Some(token) = current_token.take() {
                            results.push((token, current_name.take()));
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_token_defaults_to_profile_1() {
        let client = OnvifPtzClient::new("http://cam.local/onvif/ptz_service".into(), None, None);
        assert_eq!(client.profile_token(), "Profile_1");
    }

    #[test]
    fn with_profile_overrides_token() {
        let client = OnvifPtzClient::new("http://cam.local/onvif/ptz_service".into(), None, None)
            .with_profile("mainStream".into());
        assert_eq!(client.profile_token(), "mainStream");
    }

    #[test]
    fn continuous_move_body_carries_profile_token() {
        let client = OnvifPtzClient::new("http://cam.local/onvif/ptz_service".into(), None, None)
            .with_profile("mainStream".into());
        let body = client.continuous_move_body(0.1, 0.2, 0.3);
        assert!(body.contains("<ProfileToken>mainStream</ProfileToken>"));
        assert!(body.contains("xmlns=\"http://www.onvif.org/ver20/ptz/wsdl\""));
    }
}
