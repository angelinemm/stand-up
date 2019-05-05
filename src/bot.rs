use crate::utils::{StandUpConfig, TeamMember};
use chrono::{DateTime, Utc};
use reqwest::Client;
use slack::{api, Message as SlackMessage};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::{thread, time};

pub struct Bot {
    pub client: Client,
    pub receiver: Receiver<SlackMessage>,
    pub state: HashMap<TeamMember, State>, // stand up state by user
    pub config: StandUpConfig,
    pub cache: HashMap<TeamMember, Vec<String>>,
}

pub enum State {
    TooEarly { stand_up_time: DateTime<Utc> },
    Asked { question: u8 },
    Done,
}

impl Bot {
    pub fn new(
        client: Client,
        receiver: Receiver<SlackMessage>,
        config: StandUpConfig,
    ) -> Result<Bot, ()> {
        let stand_up_time = config.stand_up_time.today()?;
        let initial_state: HashMap<TeamMember, State> = config
            .team_members
            .iter()
            .map(|m| ((*m).clone(), State::TooEarly { stand_up_time }))
            .collect();
        let initial_cache: HashMap<TeamMember, Vec<String>> = config
            .team_members
            .iter()
            .map(|m| ((*m).clone(), Vec::new()))
            .collect();
        Ok(Bot {
            client,
            receiver,
            state: initial_state,
            config,
            cache: initial_cache,
        })
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

    fn say_hello(&self, team_member: &TeamMember) {
        self.post_message(
            &team_member.dm_id,
            &format!(
                "Hello {}, it's {}! Time for your daily stand-up!",
                team_member,
                Utc::now().format("%A"),
            ),
        );
    }

    fn question(&self, team_member: &TeamMember, question: u8) {
        let question = &self.config.questions[(question - 1) as usize];
        self.post_message(&team_member.dm_id, question);
    }

    fn post_stand_up(&mut self, team_member: &TeamMember) {
        let stand_up: Vec<String> = self.cache.remove(team_member).unwrap_or_default();
        let stand_up_message: String = (1..=self.config.number_of_questions)
            .map(|i| i as usize)
            .map(|i| format!("*{}*: {}", self.config.questions[i - 1], stand_up[i - 1]))
            .collect::<Vec<String>>()
            .join("\n");

        self.post_message(
            &self.config.channel_id,
            &format!("*{}*\n{}", team_member, stand_up_message),
        );
    }

    pub fn stand_up_machine(&mut self) {
        let ten_seconds = time::Duration::from_secs(10);
        let channel_timeout = time::Duration::from_millis(10);
        loop {
            // First, consume all messages in the channel
            while let Ok(message) = self.receiver.recv_timeout(channel_timeout) {
                self.handle_message(&message);
            }
            // Then, maybe advance state machine
            let now = Utc::now();
            for team_member in self.config.team_members.iter() {
                let state = self.state.get(&team_member);
                match state {
                    Some(State::TooEarly { stand_up_time }) => {
                        println!("STATE ({}): Too early for standup!", team_member);
                        if now > *stand_up_time {
                            println!("TRANSITION ({}): now asking first question", team_member);
                            self.say_hello(team_member);
                            self.question(team_member, 1);
                            self.state
                                .insert((*team_member).clone(), State::Asked { question: 1 });
                        }
                    }
                    Some(State::Asked { .. }) => {
                        println!("STATE ({}): Stand up has been asked", team_member);
                        if now < self.config.stand_up_time.today().unwrap() {
                            // means we are next day
                            println!("TRANSITION ({}): Day change", team_member);
                            self.state.insert(
                                (*team_member).clone(),
                                State::TooEarly {
                                    stand_up_time: self.config.stand_up_time.today().unwrap(),
                                },
                            );
                        }
                    }
                    Some(State::Done) => {
                        println!("STATE ({}): Stand up is done for the day", team_member);
                        if now < self.config.stand_up_time.today().unwrap() {
                            println!("TRANSITION ({}): Day change", team_member);
                            // means we are next day
                            self.state.insert(
                                (*team_member).clone(),
                                State::TooEarly {
                                    stand_up_time: self.config.stand_up_time.today().unwrap(),
                                },
                            );
                        }
                    }
                    None => println!("Cannot find state for {}", team_member),
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

            let team_member = self
                .config
                .team_members
                .iter()
                .find(|m| m.id == *answer_user)
                .expect("Message from unknown user")
                .clone();

            let state = self.state.get(&team_member);
            match state {
                Some(State::Asked { question: i }) => {
                    let mut answers: Vec<String> = self.cache[&team_member].to_vec();
                    answers.push(answer.to_string());
                    self.cache.insert(team_member.clone(), answers);
                    if *i == self.config.number_of_questions {
                        // It was the last question
                        println!(
                            "TRANSITION ({}): Answer to last question received. Stand up done",
                            team_member
                        );
                        self.post_stand_up(&team_member);
                        self.state.insert(team_member, State::Done);
                    } else {
                        // Next question
                        println!(
                            "TRANSITION ({}): Answer to question {} received. Next question.",
                            team_member, i
                        );
                        self.question(&team_member, i + 1);
                        self.state
                            .insert(team_member, State::Asked { question: i + 1 });
                    }
                }
                None | _ => {
                    println!("Unexpected message received. Ignoring");
                    return;
                }
            }
        }
    }
}
