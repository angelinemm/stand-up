use actix_web::Responder;

pub fn ping() -> impl Responder {
    "Pong!".to_string()
}
