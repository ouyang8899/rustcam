use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;

pub struct CronExpr {
    schedule: Schedule,
}

impl CronExpr {
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let schedule = Schedule::from_str(expr).map_err(|e| CronError::InvalidExpression(e.to_string()))?;
        Ok(Self { schedule })
    }

    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        self.schedule.upcoming(Utc).next().map(|t| t >= *dt).unwrap_or(false)
    }
}

#[derive(Debug)]
pub enum CronError {
    InvalidExpression(String),
}

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CronError::InvalidExpression(s) => write!(f, "Invalid cron expression: {}", s),
        }
    }
}

impl std::error::Error for CronError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_every_minute() {
        let expr = CronExpr::parse("0 * * * * *").unwrap();
        let dt = Utc::now();
        assert!(expr.matches(&dt));
    }

    #[test]
    fn test_parse_invalid() {
        let result = CronExpr::parse("invalid");
        assert!(result.is_err());
    }
}
