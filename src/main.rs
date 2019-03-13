mod handler;
mod stand_up;

use crate::handler::MyHandler;
use crate::stand_up::{get_slack_config, StandUp};
use config;
use slack_api::{api, Message as SlackMessage, RtmClient};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

fn main() {
    let mut config = config::Config::default();
    config.merge(config::File::with_name("Settings")).unwrap();
    let api_key: String = config.get_str("api_key").unwrap();
    let (sender, receiver): (Sender<SlackMessage>, Receiver<SlackMessage>) = mpsc::channel();
    let ws_cli = RtmClient::login(&api_key).expect("Can't login websocket client");
    let stand_up_config = get_slack_config(&ws_cli, &config);
    let web_cli = api::requests::default_client().unwrap();
    let listener_thread = thread::spawn(move || {
        let mut handler = MyHandler { sender };
        let r = ws_cli.run(&mut handler);
        match r {
            Ok(_) => {}
            Err(err) => panic!("Error: {}", err),
        }
    });
    let stand_up_bot_thread = thread::spawn(move || {
        let mut stand_up = StandUp::new(web_cli, receiver, stand_up_config);
        stand_up.stand_up_loop();
    });
    listener_thread.join().unwrap();
    stand_up_bot_thread.join().unwrap();
}
