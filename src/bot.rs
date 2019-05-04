use crate::utils::StandUpConfig;
use chrono::{DateTime, Datelike, Utc};
use reqwest::Client;
use slack_api::{api, Message as SlackMessage};
use std::sync::mpsc::Receiver;
use std::{thread, time};

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
