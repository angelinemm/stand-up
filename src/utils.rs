use chrono::{DateTime, Timelike, Utc};
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum Period {
    AM,
    PM,
}

impl Period {
    pub fn from_str(s: &str) -> Result<Period, ()> {
        match s {
            "AM" => Ok(Period::AM),
            "PM" => Ok(Period::PM),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TimeOfDay {
    pub hour: u8,
    pub min: u8,
    pub period: Period,
}

impl TimeOfDay {
    pub fn from_str(s: &str) -> Result<TimeOfDay, ()> {
        let n = s.len();
        let times: Vec<Result<u8, ()>> = s[..n - 2]
            .split(':')
            .map(|x| x.parse::<u8>().map_err(|_| ()))
            .collect();
        let hour = times[0]?;
        let min = times[1]?;
        let period = Period::from_str(&s[n - 2..])?;
        Ok(Self { hour, min, period })
    }

    pub fn today(&self) -> Result<DateTime<Utc>, ()> {
        let (hour, min) = match (&self.period, &self.hour) {
            (Period::AM, 12) => (0, self.min),
            (Period::PM, 12) => (self.hour, self.min),
            (Period::AM, _) => (self.hour, self.min),
            (Period::PM, _) => (self.hour + 12, self.min),
        };
        Utc::now()
            .with_hour(hour.into())
            .ok_or(())?
            .with_minute(min.into())
            .ok_or(())?
            .with_second(0)
            .ok_or(())?
            .with_nanosecond(0)
            .ok_or(())
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct TeamMember {
    pub name: String,
    pub id: String,
    pub dm_id: String,
}

impl fmt::Display for TeamMember {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_stand_up_time() {
        assert_eq!(
            TimeOfDay::from_str("9:00AM"),
            Ok(TimeOfDay {
                hour: 9,
                min: 0,
                period: Period::AM
            })
        );
        assert_eq!(
            TimeOfDay::from_str("6:30PM"),
            Ok(TimeOfDay {
                hour: 6,
                min: 30,
                period: Period::PM
            })
        );

        assert!(TimeOfDay::from_str("blah").is_err());
    }

    #[test]
    fn stand_up_time_building() {
        let nine_am = TimeOfDay::from_str("9:00AM").unwrap().today().unwrap();
        assert_eq!(nine_am.hour(), 9);
        assert_eq!(nine_am.minute(), 0);

        let ten_thirty_pm = TimeOfDay::from_str("10:00PM").unwrap().today().unwrap();
        assert_eq!(ten_thirty_pm.hour(), 22);
        assert_eq!(ten_thirty_pm.minute(), 0);

        let midnight = TimeOfDay::from_str("12:00AM").unwrap().today().unwrap();
        assert_eq!(midnight.hour(), 0);
        assert_eq!(midnight.minute(), 0);

        let noon = TimeOfDay::from_str("12:00PM").unwrap().today().unwrap();
        assert_eq!(noon.hour(), 12);
        assert_eq!(noon.minute(), 0);
    }
}
