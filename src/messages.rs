use serde::{Serialize, Deserialize};

/// A message that is passed between the websockets and to the master
/// A message describes the result of a command and it also describes any errors that can occur
/// Example:
/// ```json
/// {
///     statusCode: 200,
///     message: MessageType::ACK,
///     pet_action_required: null
/// }
/// ``` 
/// ```json
/// Or
/// {
///     statusCode: 400,
///     message: MessageTyp::InvalidParams,
///     pet_action_required: null
/// }
/// ``` 
/// Or
/// /// {
///     statusCode: 500,
///     message: MessageTyp::PetOffline,
///     pet_action_required: "Reconnect or reboot collar"
/// }
/// ``` 
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Message{
    status_code: i16,
    message: String,
    pet_action_required: Option<String>,
}

impl Message {
    pub fn new(status_code: i16, message: MessageTyp, pet_action_required: ActionType) -> Message {
        Message { status_code, message: message.to_string(), pet_action_required: pet_action_required.to_string() }
    }
    pub fn get_type(&self) -> MessageTyp{
        MessageTyp::from_string(&self.message)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum MessageTyp {
    ACK,
    InvalidParams,
    InvalidCommand,
    PetOffline,
    PetUnrecoverableError,
    PetOnline,
    KeepAlive
}

impl MessageTyp {
    fn to_string(&self) -> String {
        match *self {
            MessageTyp::ACK => "ack".to_string(),
            MessageTyp::InvalidParams =>"Invalid parameters".to_string(),
            MessageTyp::InvalidCommand => "Invalid command".to_string(),
            MessageTyp::PetOffline => "Pet went offline".to_string(),
            MessageTyp::PetUnrecoverableError => "Pet went offline with an unrecoverable error".to_string(),
            MessageTyp::PetOnline => "Pet is online".to_string(),
            MessageTyp::KeepAlive => "Awaiting ACK to confirm Keep alive response".to_string(),
        }
    }
    fn from_string(s: &String) -> MessageTyp{
        match s.as_str() {
        "ack" => MessageTyp::ACK,
        "Invalid params" => MessageTyp::InvalidParams,
        "Invalid command" => MessageTyp::InvalidCommand,
        "Pet offline" => MessageTyp::PetOffline,
        "Pet went offline with an unrecoverable error" => MessageTyp::PetUnrecoverableError,
        _ => unreachable!()
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ActionType {
    Reboot,
    Reconnect,
    RebootReconnect,
    None
}

impl ActionType {
    fn to_string(&self) -> Option<String> {
        match *self {
            ActionType::Reboot => Some("Request reboot from pet".to_string()),
            ActionType::Reconnect => Some("Request reconnect from pet".to_string()),
            ActionType::RebootReconnect => Some("Request reconnect and reboot from pet".to_string()),
            ActionType::None => None,
        }
    }
}