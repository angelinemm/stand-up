use chrono::{DateTime, Datelike, Timelike, Utc};
use config::Config;
use reqwest::Client;
use slack_api::{api, Message as SlackMessage, RtmClient, User as SlackUser};
use std::sync::mpsc::Receiver;
use std::{thread, time};

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

pub struct TeamMember {
    pub name: String,
    pub id: String,
    pub dm_id: String,
}

pub struct Bot {
    pub client: Client,
    pub receiver: Receiver<SlackMessage>,
    pub state: BotState,
    pub config: StandUpConfig,
}

pub enum BotState {
    TooEarly { stand_up_time: DateTime<Utc> },
    Asked,
    Done,
}

impl Bot {
    pub fn new(client: Client, receiver: Receiver<SlackMessage>, config: StandUpConfig) -> Bot {
        Bot {
            client,
            receiver,
            state: BotState::TooEarly {
                stand_up_time: config.stand_up_time.today().unwrap(),
            },
            config,
        }
    }

    fn post_message(&self, channel_id: &str, message: &str) {
        let _ = api::chat::post_message(
            &self.client,
            &self.config.api_key,
            &api::chat::PostMessageRequest {
                channel: channel_id,
                text: message,
                ..api::chat::PostMessageRequest::default()
            },
        );
    }

    fn say_hello(&self) {
        let day = Utc::now().weekday();
        for member in self.config.team_members.iter() {
            self.post_message(
                &member.dm_id,
                &format!(
                    "Hello {}, it's {:?}day! Time for your daily stand-up!",
                    member.name, day
                ),
            );
        }
    }

    fn q1(&self) {
        for member in self.config.team_members.iter() {
            self.post_message(
                &member.dm_id,
                &format!("So {}, what were you up to yesterday?", member.name),
            );
        }
    }

    pub fn stand_up_machine(&mut self) {
        let ten_seconds = time::Duration::from_secs(10);
        let channel_timeout = time::Duration::from_millis(10);
        let now = Utc::now();
        loop {
            match self.state {
                BotState::TooEarly { stand_up_time } => {
                    println!("STATE: Too early for standup!");
                    if now > stand_up_time {
                        println!("TRANSITION: now asking stand up");
                        self.say_hello();
                        self.q1();
                        self.state = BotState::Asked;
                    }
                }
                BotState::Asked => {
                    println!("STATE: Stand up has been asked");
                    if let Ok(message) = self.receiver.recv_timeout(channel_timeout) {
                        self.handle_message(&message);
                    }
                    if now < self.config.stand_up_time.today().unwrap() {
                        // means we are next day
                        println!("TRANSITION: Day change");
                        self.state = BotState::TooEarly {
                            stand_up_time: self.config.stand_up_time.today().unwrap(),
                        }
                    }
                }
                BotState::Done => {
                    println!("STATE: Stand up is done for the day");
                    if now < self.config.stand_up_time.today().unwrap() {
                        println!("TRANSITION: Day change");
                        // means we are next day
                        self.state = BotState::TooEarly {
                            stand_up_time: self.config.stand_up_time.today().unwrap(),
                        }
                    }
                }
            }
            thread::sleep(ten_seconds);
        }
    }

    pub fn handle_message(&mut self, message: &SlackMessage) {
        if let SlackMessage::Standard(msg) = message {
            // Ignore all messages that are not DMs from team members
            match &msg.channel {
                None => return,
                Some(id) => {
                    if !self.config.team_members.iter().any(|m| m.dm_id == *id) {
                        return;
                    }
                }
            }

            let answer = match &msg.text {
                None => "Nothing",
                Some(text) => text,
            };

            let answer_user = match &msg.user {
                None => panic!("Message with no user"),
                Some(identity) => identity,
            };

            let user = self
                .config
                .team_members
                .iter()
                .find(|user| user.id == *answer_user)
                .expect("Message from unknown user");

            self.post_message(
                &self.config.channel_id,
                &format!("{}: {}", user.name, answer),
            );
            println!("TRANSITION: Message received, standup done for today");
            self.state = BotState::Done;
        }
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
