use chrono::{DateTime, Timelike, Utc};
use config::Config;
use slack_api::{RtmClient, User as SlackUser};
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
            .ok_or(())
    }
}

pub struct StandUpConfig {
    pub api_key: String,
    pub channel_id: String,
    pub team_members: Vec<TeamMember>,
    pub stand_up_time: TimeOfDay,
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

pub fn get_stand_up_config(client: &RtmClient, config: &Config) -> StandUpConfig {
    let api_key: String = config.get_str("api_key").unwrap();
    let channel: String = config.get_str("channel").unwrap();
    let stand_up_time = TimeOfDay::from_str(&config.get_str("stand_up_time").unwrap()).unwrap();
    let team_members: Vec<String> = config
        .get_str("team_members")
        .unwrap()
        .split(',')
        .map(|s| s.to_string())
        .collect();
    let channel_id = client
        .start_response()
        .channels
        .as_ref()
        .and_then(|channels| {
            channels.iter().find(|chan| match chan.name {
                None => false,
                Some(ref name) => name == &channel,
            })
        })
        .and_then(|chan| chan.id.clone())
        .expect("Could not find channel for stand-up :( ");
    let users: Vec<&SlackUser> = client
        .start_response()
        .users
        .as_ref()
        .expect("No users found")
        .iter()
        .filter(|user| match user.name {
            None => false,
            Some(ref name) => team_members.contains(name),
        })
        .collect();
    let team_members: Vec<TeamMember> = client
        .start_response()
        .ims
        .as_ref()
        .expect("No direct messages found")
        .iter()
        .filter_map(|dm| match users.iter().find(|user| user.id == dm.user) {
            None => None,
            Some(ref user) => Some(TeamMember {
                name: user
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unknown name".to_string()),
                id: user.id.clone().expect("User without an id"),
                dm_id: dm.id.clone().expect("DM without an id"),
            }),
        })
        .collect();
    StandUpConfig {
        api_key,
        channel_id,
        team_members,
        stand_up_time,
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
