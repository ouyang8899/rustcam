//! GB/T 28181 附录 A.3 前端设备控制协议编码。
//!
//! 8 字节指令格式：
//! `A5 | 组合码1 | 地址低8位 | 指令码 | 数据1 | 数据2 | 组合码2 | 校验码`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetAction {
    Set,
    Call,
    Remove,
}

impl PresetAction {
    fn cmd_byte(self) -> u8 {
        match self {
            PresetAction::Set => 0x81,
            PresetAction::Call => 0x82,
            PresetAction::Remove => 0x83,
        }
    }
}

/// 将 API 的 0..=100 速度映射为协议要求的 0..=255。
pub fn speed_from_percent(speed: u8) -> u8 {
    if speed == 0 {
        return 0;
    }
    (((speed as u32) * 255 + 50) / 100).clamp(1, 255) as u8
}

fn checksum(bytes: &[u8; 8]) -> u8 {
    bytes[..7].iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

fn finish(mut bytes: [u8; 8]) -> String {
    bytes[7] = checksum(&bytes);
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

fn base(cmd: u8) -> [u8; 8] {
    // 组合码1 = (A + 5 + 0) % 16 = 0x0F，地址低 8 位取 0x01。
    [0xA5, 0x0F, 0x01, cmd, 0, 0, 0, 0]
}

fn zoom_byte(speed: u8) -> u8 {
    // 字节7 高 4 位为变倍速度，低 4 位为地址高 4 位。
    ((speed >> 4).max(1) & 0x0F) << 4
}

/// 云台/变倍/聚焦/光圈方向指令。
///
/// 方向码：
/// - 右 0x01、左 0x02、下 0x04、上 0x08
/// - 变倍放大 0x10、变倍缩小 0x20
/// - 聚焦远 0x41、聚焦近 0x42、光圈放大 0x44、光圈缩小 0x48
pub fn encode_direction(direction: &str, speed: u8) -> String {
    let mut bytes = base(0);
    match direction {
        "up" => {
            bytes[3] = 0x08;
            bytes[5] = speed;
        }
        "down" => {
            bytes[3] = 0x04;
            bytes[5] = speed;
        }
        "left" => {
            bytes[3] = 0x02;
            bytes[4] = speed;
        }
        "right" => {
            bytes[3] = 0x01;
            bytes[4] = speed;
        }
        "zoom_in" => {
            bytes[3] = 0x10;
            bytes[6] = zoom_byte(speed);
        }
        "zoom_out" => {
            bytes[3] = 0x20;
            bytes[6] = zoom_byte(speed);
        }
        "focus_far" | "focus_out" => {
            bytes[3] = 0x41;
            bytes[4] = speed;
        }
        "focus_near" | "focus_in" => {
            bytes[3] = 0x42;
            bytes[4] = speed;
        }
        "iris_open" | "iris_in" => {
            bytes[3] = 0x44;
            bytes[5] = speed;
        }
        "iris_close" | "iris_out" => {
            bytes[3] = 0x48;
            bytes[5] = speed;
        }
        _ => {}
    }
    finish(bytes)
}

/// PTZ 连续移动组合指令。pan/tilt/zoom 取值范围 -1.0..=1.0。
pub fn encode_compound(pan: f64, tilt: f64, zoom: f64) -> String {
    let mut bytes = base(0);
    let speed = |v: f64| -> u8 {
        if v.abs() <= 0.01 {
            0
        } else {
            ((v.abs() * 255.0) as u32).clamp(1, 255) as u8
        }
    };

    let pan_speed = speed(pan);
    let tilt_speed = speed(tilt);
    let zoom_speed = speed(zoom);

    if pan > 0.01 {
        bytes[3] |= 0x01;
    }
    if pan < -0.01 {
        bytes[3] |= 0x02;
    }
    if tilt < -0.01 {
        bytes[3] |= 0x04;
    }
    if tilt > 0.01 {
        bytes[3] |= 0x08;
    }
    if zoom > 0.01 {
        bytes[3] |= 0x10;
    }
    if zoom < -0.01 {
        bytes[3] |= 0x20;
    }

    bytes[4] = pan_speed;
    bytes[5] = tilt_speed;
    if zoom_speed > 0 {
        bytes[6] = zoom_byte(zoom_speed);
    }

    finish(bytes)
}

/// 预置位设置/调用/删除指令。
pub fn encode_preset(action: PresetAction, index: u8) -> String {
    let mut bytes = base(action.cmd_byte());
    bytes[5] = index;
    finish(bytes)
}

/// 构造标准的 DeviceControl XML 请求体。
pub fn build_device_control_xml(channel_id: &str, sn: u32, ptz_cmd: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Control><CmdType>DeviceControl</CmdType><SN>{}</SN><DeviceID>{}</DeviceID><PTZCmd>{}</PTZCmd></Control>"#,
        sn, channel_id, ptz_cmd
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_directions() {
        let cases = [
            ("up", 0x1F, "A50F0108001F00DC"),
            ("down", 0x1F, "A50F0104001F00D8"),
            ("left", 0x1F, "A50F01021F0000D6"),
            ("right", 0x1F, "A50F01011F0000D5"),
            ("zoom_in", 0x10, "A50F0110000010D5"),
            ("zoom_out", 0x10, "A50F0120000010E5"),
            ("stop", 0x00, "A50F0100000000B5"),
        ];
        for (direction, speed, expected) in cases {
            assert_eq!(encode_direction(direction, speed), expected, "{}", direction);
        }
    }

    #[test]
    fn golden_presets() {
        assert_eq!(encode_preset(PresetAction::Set, 1), "A50F018100010037");
        assert_eq!(encode_preset(PresetAction::Call, 1), "A50F018200010038");
        assert_eq!(encode_preset(PresetAction::Remove, 1), "A50F018300010039");
    }

    #[test]
    fn compound_up_right() {
        let cmd = encode_compound(1.0, 1.0, 0.0);
        assert_eq!(&cmd[6..8], "09");
        assert_eq!(&cmd[8..10], "FF");
        assert_eq!(&cmd[10..12], "FF");
        assert_eq!(cmd.len(), 16);
    }

    #[test]
    fn compound_all_stop() {
        assert_eq!(encode_compound(0.0, 0.0, 0.0), "A50F0100000000B5");
    }

    #[test]
    fn xml_uses_device_control() {
        let xml = build_device_control_xml("34020000001310000001", 7, "A50F0108001F00DC");
        assert!(xml.contains("<Control>"));
        assert!(xml.contains("<CmdType>DeviceControl</CmdType>"));
        assert!(xml.contains("<SN>7</SN>"));
        assert!(xml.contains("<DeviceID>34020000001310000001</DeviceID>"));
        assert!(xml.contains("<PTZCmd>A50F0108001F00DC</PTZCmd>"));
        assert!(!xml.contains("<Notify>"));
    }
}
