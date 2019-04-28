use chrono::{DateTime, Datelike, Utc};
use config::Config;
use reqwest::Client;
use slack_api::{api, Message as SlackMessage, RtmClient, User as SlackUser};
use std::sync::mpsc::Receiver;
use std::{thread, time};

pub fn get_slack_config(client: &RtmClient, config: &Config) -> SlackConfig {
    let api_key: String = config.get_str("api_key").unwrap();
    let channel: String = config.get_str("channel").unwrap();
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
    SlackConfig {
        api_key,
        channel_id,
        team_members,
    }
}

pub struct SlackConfig {
    pub api_key: String,
    pub channel_id: String,
    pub team_members: Vec<TeamMember>,
}

pub struct TeamMember {
    pub name: String,
    pub id: String,
    pub dm_id: String,
}

pub struct Bot {
    pub client: Client,
    pub api_key: String,
    pub receiver: Receiver<SlackMessage>,
    pub channel_id: String,
    pub team_members: Vec<TeamMember>,
    pub state: BotState,
}

pub enum BotState {
    NotAskedYet,
    Asked { last_asked: DateTime<Utc> },
}

impl Bot {
    pub fn new(client: Client, receiver: Receiver<SlackMessage>, slack_config: SlackConfig) -> Bot {
        Bot {
            client,
            api_key: slack_config.api_key,
            receiver,
            channel_id: slack_config.channel_id,
            team_members: slack_config.team_members,
            state: BotState::NotAskedYet,
        }
    }

    fn asked_today(&self) -> bool {
        match self.state {
            BotState::NotAskedYet => false,
            BotState::Asked { last_asked } => {
                let now = Utc::now();
                (now.year() == last_asked.year()
                    && now.month() == last_asked.month()
                    && now.day() == last_asked.day())
            }
        }
    }

    fn post_message(&self, channel_id: &str, message: &str) {
        let _ = api::chat::post_message(
            &self.client,
            &self.api_key,
            &api::chat::PostMessageRequest {
                channel: channel_id,
                text: message,
                ..api::chat::PostMessageRequest::default()
            },
        );
    }

    fn say_hello(&self) {
        let day = Utc::now().weekday();
        for member in self.team_members.iter() {
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
        for member in self.team_members.iter() {
            self.post_message(
                &member.dm_id,
                &format!("So {}, what were you up to yesterday?", member.name),
            );
        }
    }

    pub fn stand_up_machine(&mut self) {
        let ten_seconds = time::Duration::from_secs(10);
        let channel_timeout = time::Duration::from_millis(10);
        loop {
            match self.state {
                BotState::NotAskedYet => {
                    self.say_hello();
                    self.q1();
                    self.state = BotState::Asked {
                        last_asked: Utc::now(),
                    }
                }
                BotState::Asked { last_asked: _ } => match self.asked_today() {
                    true => match self.receiver.recv_timeout(channel_timeout) {
                        Ok(message) => self.handle_message(&message),
                        Err(_) => {}
                    },
                    false => self.state = BotState::NotAskedYet,
                },
            }
            thread::sleep(ten_seconds);
        }
    }

    pub fn handle_message(&self, message: &SlackMessage) {
        if let SlackMessage::Standard(msg) = message {
            // Ignore all messages that are not DMs from team members
            match &msg.channel {
                None => return,
                Some(id) => {
                    if !self.team_members.iter().any(|m| m.dm_id == *id) {
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
                .team_members
                .iter()
                .find(|user| user.id == *answer_user)
                .expect("Message from unknown user");

            self.post_message(&self.channel_id, &format!("{}: {}", user.name, answer));
        }
    }
}
