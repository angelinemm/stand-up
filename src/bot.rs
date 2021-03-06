use crate::config::Config;
use crate::utils::TeamMember;
use chrono::{DateTime, Utc};
use reqwest::Client;
use slack::{api, Message as SlackMessage};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::{thread, time};

static CHANNEL_TIMEOUT: time::Duration = time::Duration::from_millis(10);
static SLEEP_TIME: time::Duration = time::Duration::from_secs(10);

pub struct Bot {
    pub client: Client,
    pub receiver: Receiver<SlackMessage>,
    pub state: HashMap<TeamMember, State>, // stand up state by user
    pub config: Config,
    pub cache: HashMap<TeamMember, Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum State {
    TooEarly { stand_up_time: DateTime<Utc> },
    Asked { question: u8 },
    Done,
}

impl Bot {
    pub fn new(
        client: Client,
        receiver: Receiver<SlackMessage>,
        config: Config,
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

    fn say_goodbye(&self, team_member: &TeamMember) {
        self.post_message(
            &team_member.dm_id,
            "All done for the day, speak to you tomorrow!",
        );
    }

    fn question(&self, team_member: &TeamMember, question: u8) {
        let question = &self.config.questions[(question - 1) as usize];
        self.post_message(&team_member.dm_id, question);
    }

    fn post_stand_up(&mut self, team_member: &TeamMember) {
        let stand_up: Vec<String> = self.cache[team_member].to_vec();
        self.cache.insert(team_member.clone(), Vec::new());

        let stand_up_message: String = stand_up
            .iter()
            .enumerate()
            .map(|(idx, answer)| format!("*{}*: {}", self.config.questions[idx], answer))
            .collect::<Vec<String>>()
            .join("\n");

        self.post_message(
            &self.config.channel_id,
            &format!("*{}*\n{}", team_member, stand_up_message),
        );
    }

    pub fn maybe_advance_machine(
        &mut self,
        machine_time: DateTime<Utc>,
        stand_up_time: DateTime<Utc>,
    ) {
        for team_member in self.config.team_members.iter() {
            let state = self.state.get(&team_member);
            match state {
                Some(State::TooEarly { stand_up_time }) => {
                    if machine_time > *stand_up_time {
                        println!("TRANSITION ({}): now asking first question", team_member);
                        self.say_hello(team_member);
                        self.question(team_member, 1);
                        self.state
                            .insert((*team_member).clone(), State::Asked { question: 1 });
                    }
                }
                Some(State::Asked { .. }) | Some(State::Done) => {
                    if machine_time < stand_up_time {
                        println!("TRANSITION ({}): Day change", team_member);
                        self.cache.insert(team_member.clone(), Vec::new());
                        self.state
                            .insert((*team_member).clone(), State::TooEarly { stand_up_time });
                    }
                }
                None => println!("Cannot find state for {}", team_member),
            }
        }
        // Last, consume all messages in the channel
        while let Ok(message) = self.receiver.recv_timeout(CHANNEL_TIMEOUT) {
            self.handle_message(&message);
        }
    }

    pub fn stand_up_machine(&mut self) {
        loop {
            let now = Utc::now();
            let todays_standup = self
                .config
                .stand_up_time
                .today()
                .expect("Could not find stand up time for today");
            self.maybe_advance_machine(now, todays_standup);
            thread::sleep(SLEEP_TIME);
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

            let state = self.state.clone();
            let member_state = state.get(&team_member);
            let last_question = self.config.questions.len() as u8;
            match member_state {
                Some(State::Asked { question: i }) => {
                    let mut answers: Vec<String> = self.cache[&team_member].to_vec();
                    answers.push(answer.to_string());
                    self.cache.insert(team_member.clone(), answers);
                    if *i == last_question {
                        // It was the last question
                        println!(
                            "TRANSITION ({}): Answer to last question received. Stand up done",
                            team_member
                        );
                        self.say_goodbye(&team_member);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{TeamMember, TimeOfDay};
    use chrono::Duration;
    use std::sync::mpsc;

    #[test]
    fn stand_up_story() {
        let bob = TeamMember {
            name: "bob".to_string(),
            id: "123xx".to_string(),
            dm_id: "dm_id".to_string(),
        };
        let config = Config {
            api_key: "mock".to_string(),
            channel_id: "dummy".to_string(),
            team_members: vec![bob.clone()],
            stand_up_time: TimeOfDay::from_str("9:00AM").unwrap(),
            questions: vec!["What's up?".to_string()],
        };
        let client = Client::new();
        let (_, receiver): (_, Receiver<SlackMessage>) = mpsc::channel();
        let mut bot = Bot::new(client, receiver, config).unwrap();

        // Check initial state is "too early"
        assert_matches!(*bot.state.get(&bob).unwrap(), State::TooEarly{ stand_up_time: _ });

        // stand up time has passed, check state is question asked
        let mut now = Utc::now();
        let mut stand_up_time = now - Duration::minutes(10);
        bot.maybe_advance_machine(now, stand_up_time);
        assert_eq!(bot.state.get(&bob).unwrap(), &State::Asked { question: 1 });

        // a day has passed!
        now = now + Duration::days(1);
        stand_up_time = now + Duration::days(1);
        bot.maybe_advance_machine(now, stand_up_time);
        assert_matches!(*bot.state.get(&bob).unwrap(), State::TooEarly{ stand_up_time: _ });
    }
}
