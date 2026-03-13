use serde::Serialize;

#[derive(Serialize)]
pub struct WaybarOutput {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u8>,
}
