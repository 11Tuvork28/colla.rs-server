use serde::{Serialize, Deserialize};

/// A message that is passed between the websockets and to the master
/// A message describes the result of a command and it also describes any errors that can occur
/// Example:
/// ```json
/// {
///     statusCode: 200,
///     message: "Send to pet",
///     pet_action_required: null
/// }
/// ``` 
/// ```json
/// Or
/// {
///     statusCode: 400,
///     message: "Invalid params",
///     pet_action_required: null
/// }
/// ``` 
/// Or
/// /// {
///     statusCode: 500,
///     message: "Pet disconnected",
///     pet_action_required: "Reconnect or reboot collar"
/// }
/// ``` 
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Message{
    pub status_code: i16,
    pub message: String,
    pub pet_action_required: Option<String>,
}