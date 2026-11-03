use std::collections::BTreeMap;

/// 手写 Prometheus 文本输出；格式与主服务 `monitoring::Metrics` 一致（前缀 `rustcam_drone_`）。
#[derive(Debug, Default)]
pub struct MetricsText {
    counters: BTreeMap<String, u64>,
    gauges: BTreeMap<String, f64>,
}

impl MetricsText {
    pub fn inc(&mut self, name: &str) {
        *self.counters.entry(name.to_string()).or_insert(0) += 1;
    }

    pub fn set_gauge(&mut self, name: &str, value: f64) {
        self.gauges.insert(name.to_string(), value);
    }

    pub fn counter(&self, name: &str) -> u64 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, value) in &self.counters {
            out.push_str(&format!("# TYPE {name} counter\n{name} {value}\n"));
        }
        for (name, value) in &self.gauges {
            out.push_str(&format!("# TYPE {name} gauge\n{name} {value}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_prometheus_text() {
        let mut m = MetricsText::default();
        m.inc("rustcam_drone_adapter_connections_total");
        m.set_gauge("rustcam_drone_ws_subscribers", 3.0);
        let text = m.render();
        assert!(text.contains("rustcam_drone_adapter_connections_total 1"));
        assert!(text.contains("rustcam_drone_ws_subscribers 3"));
        assert!(text.contains("# TYPE rustcam_drone_ws_subscribers gauge"));
    }

    #[test]
    fn unknown_counter_is_zero() {
        let m = MetricsText::default();
        assert_eq!(m.counter("nope"), 0);
    }
}
