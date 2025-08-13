use super::*;


// --- Data Structures ---
#[derive(Deserialize, Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Invitation {
    pub id: Uuid,
    pub group_name: String,
    pub inviter_username: String,
}

#[derive(Serialize)]
pub struct WsClientMessage {
    pub content: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WsServerMessage {
    pub sender_id: Uuid,
    pub sender_username: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
    pub groups: Vec<Group>,
}

// --- Messages between UI and Backend Thread ---
pub enum ToBackend {
    Register(String, String),
    Login(String, String),
    Logout,
    CreateGroup(String),
    JoinGroup(Group),
    LeaveGroup(Uuid),
    InviteUser(Uuid, String),
    SendMessage(Uuid, String),
    FetchInvitations,
    AcceptInvitation(Uuid),
    DeclineInvitation(Uuid),
    FetchGroupMessages(Uuid),
    FetchGroupMembers(Uuid),
}

#[derive(Debug)]
pub enum FromBackend {
    LoggedIn(User, String, Vec<Group>),
    Registered,
    GroupJoined(Group),
    GroupLeft(Uuid),
    NewMessage(Uuid, WsServerMessage),
    Info(String),
    Error(String),
    InvitationsFetched(Vec<Invitation>),
    InvitationDeclined(Uuid),
    GroupCreated(Group),
    GroupMessagesFetched(Uuid, Vec<WsServerMessage>),
    GroupMembersFetched(Uuid, Vec<User>),
    GroupMembersChanged,
}

#[derive(PartialEq)]
pub enum AuthState {
    Login,
    Register,
}
