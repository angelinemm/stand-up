use slack_api::{Event, EventHandler, Message as SlackMessage, RtmClient};
use std::sync::mpsc::Sender;

pub struct MyHandler {
    pub sender: Sender<SlackMessage>,
}

impl EventHandler for MyHandler {
    fn on_event(&mut self, _cli: &RtmClient, event: Event) {
        if let Event::Message(message) = event {
            self.sender.send(Box::leak(message).clone()).unwrap();
        }
    }

    fn on_close(&mut self, _cli: &RtmClient) {
        println!("on_close");
    }

    fn on_connect(&mut self, _cli: &RtmClient) {
        println!("on_connect");
    }
}
