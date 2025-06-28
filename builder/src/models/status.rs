use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize,PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Error,
    Success,
    Pending,
    Building,
    Full,
    AlreadyBuilding,
    AlreadyQueue,
    Aborted,
    NotFound,
    SomethingWentWrong,
    Unauthorized,
    MissingUniqueId,
    MaxPending,
    MissingPayload,
    FileCreateFailed,
    MissingProjectToken,
    StartingCommand,
    ChangeProjectToken,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::ChangeProjectToken => "change_project_token",
            Status::StartingCommand => "starting_command",
            Status::FileCreateFailed => "file_create_failed",
            Status::MissingPayload => "missing_payload",
            Status::Error => "error",
            Status::Success => "success",
            Status::Pending => "pending",
            Status::Building => "building",
            Status::Full => "full",
            Status::AlreadyBuilding => "already_building",
            Status::AlreadyQueue => "already_queue",
            Status::Aborted => "aborted",
            Status::NotFound => "not_found",
            Status::SomethingWentWrong => "something_went_wrong",
            Status::Unauthorized => "unauthorized",
            Status::MissingUniqueId => "missing_unique_id",
            Status::MaxPending => "max_pending",
            Status::MissingProjectToken => "missing_project_token",
        }
    } //as_str
}
