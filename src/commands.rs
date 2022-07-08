use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
// TODO: properly name these modes, I don't know them all -JSKitty
pub enum Modes {
    Zappy = 4,
    Vibey = 3,
    Beep = 2,
    Led = 1
}

impl std::fmt::Display for Modes {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl  Modes {
    pub fn as_num(&self) -> i16{
        match *self {
            Modes::Zappy => 4,
            Modes::Vibey => 3,
            Modes::Beep => 2,
            Modes::Led => 1,
        }
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Command {
    pub mode: Modes,  // 1-4
    pub level: i8,    // 1-100
    pub duration: i16 // 1-1000
}

pub fn check_validity(cmd: Command) -> bool {
    // NOTE: Mode (and other primitive checks) are already done by Serde
    cmd.level > 0 && cmd.level <= 100 && cmd.duration > 0 && cmd.duration <= 1000
}