use super::*;
#[cfg(test)]
use serial_test_derive::serial;

/// Access to the steam matchmaking interface
pub struct Matchmaking<Manager> {
    pub(crate) mm: *mut sys::ISteamMatchmaking,
    pub(crate) inner: Arc<Inner<Manager>>,
}

const CALLBACK_BASE_ID: i32 = 500;

/// The visibility of a lobby
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LobbyType {
    Private,
    FriendsOnly,
    Public,
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LobbyId(pub(crate) u64);

impl LobbyId {
    /// Creates a `LobbyId` from a raw 64 bit value.
    ///
    /// May be useful for deserializing lobby ids from
    /// a network or save format.
    pub fn from_raw(id: u64) -> LobbyId {
        LobbyId(id)
    }

    /// Returns the raw 64 bit value of the lobby id
    ///
    /// May be useful for serializing lobby ids over a
    /// network or to a save format.
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl<Manager> Matchmaking<Manager> {
    pub fn request_lobby_list<F>(&self, cb: F)
    where
        F: FnOnce(SResult<Vec<LobbyId>>) + 'static + Send,
    {
        unsafe {
            let api_call = sys::SteamAPI_ISteamMatchmaking_RequestLobbyList(self.mm);
            register_call_result::<sys::LobbyMatchList_t, _, _>(
                &self.inner,
                api_call,
                CALLBACK_BASE_ID + 10,
                move |v, io_error| {
                    cb(if io_error {
                        Err(SteamError::IOFailure)
                    } else {
                        let mut out = Vec::with_capacity(v.m_nLobbiesMatching as usize);
                        for idx in 0..v.m_nLobbiesMatching {
                            out.push(LobbyId(sys::SteamAPI_ISteamMatchmaking_GetLobbyByIndex(
                                sys::SteamAPI_SteamMatchmaking_v009(),
                                idx as _,
                            )));
                        }
                        Ok(out)
                    })
                },
            );
        }
    }

    pub fn set_distancefilter_for_lobby_list_worldwide(&self) {
        unsafe {
            sys::SteamAPI_ISteamMatchmaking_AddRequestLobbyListDistanceFilter(self.mm,
                sys::ELobbyDistanceFilter::k_ELobbyDistanceFilterWorldwide);
        }
    }

    pub fn set_slotsavailablefilter_for_lobby_list(&self, n_slots_available: i32) {
        unsafe {
            sys::SteamAPI_ISteamMatchmaking_AddRequestLobbyListFilterSlotsAvailable(self.mm, n_slots_available);
        }
    }

    pub fn set_stingfilterequals_for_lobby_list(&self, key: &str, value: &str) {
        unsafe {
            let pch_key_to_match = CString::new(key).unwrap();
            let pch_value_to_match = CString::new(value).unwrap();
            sys::SteamAPI_ISteamMatchmaking_AddRequestLobbyListStringFilter(self.mm,
                pch_key_to_match.as_ptr()  as *const _,
                pch_value_to_match.as_ptr()  as *const _,
                            sys::ELobbyComparison::k_ELobbyComparisonEqual);
        }
    }

    /// Attempts to create a new matchmaking lobby
    ///
    /// The lobby with have the visibility of the of the passed
    /// `LobbyType` and a limit of `max_members` inside it.
    /// The `max_members` may not be higher than 250.
    ///
    /// # Triggers
    ///
    /// * `LobbyEnter`
    /// * `LobbyCreated`
    pub fn create_lobby<F>(&self, ty: LobbyType, max_members: u32, cb: F)
    where
        F: FnOnce(SResult<LobbyId>) + 'static + Send,
    {
        assert!(max_members <= 250); // Steam API limits
        unsafe {
            let ty = match ty {
                LobbyType::Private => sys::ELobbyType::k_ELobbyTypePrivate,
                LobbyType::FriendsOnly => sys::ELobbyType::k_ELobbyTypeFriendsOnly,
                LobbyType::Public => sys::ELobbyType::k_ELobbyTypePublic,
                LobbyType::Invisible => sys::ELobbyType::k_ELobbyTypeInvisible,
            };
            let api_call =
                sys::SteamAPI_ISteamMatchmaking_CreateLobby(self.mm, ty, max_members as _);
            register_call_result::<sys::LobbyCreated_t, _, _>(
                &self.inner,
                api_call,
                CALLBACK_BASE_ID + 13,
                move |v, io_error| {
                    cb(if io_error {
                        Err(SteamError::IOFailure)
                    } else if v.m_eResult != sys::EResult::k_EResultOK {
                        Err(v.m_eResult.into())
                    } else {
                        Ok(LobbyId(v.m_ulSteamIDLobby))
                    })
                },
            );
        }
    }

    /// Tries to join the lobby with the given ID
    pub fn join_lobby<F>(&self, lobby: LobbyId, cb: F)
    where
        F: FnOnce(Result<LobbyId, ()>) + 'static + Send,
    {
        unsafe {
            let api_call = sys::SteamAPI_ISteamMatchmaking_JoinLobby(self.mm, lobby.0);
            register_call_result::<sys::LobbyEnter_t, _, _>(
                &self.inner,
                api_call,
                CALLBACK_BASE_ID + 4,
                move |v, io_error| {
                    cb(if io_error {
                        Err(())
                    } else if v.m_EChatRoomEnterResponse != 1 {
                        Err(())
                    } else {
                        Ok(LobbyId(v.m_ulSteamIDLobby))
                    })
                },
            );
        }
    }

    /// Returns the lobby metadata associated with the specified key from the
    /// specified lobby.
    pub fn lobby_data(&self, lobby: LobbyId, key: &str) -> Option<&str> {
        let key = CString::new(key).unwrap();
        let data = unsafe {
            let data = sys::SteamAPI_ISteamMatchmaking_GetLobbyData(self.mm, lobby.0, key.as_ptr());
            let data = CStr::from_ptr(data);

            data
        };

        let data = data.to_str().unwrap();

        match data.is_empty() {
            false => Some(data),
            true => None,
        }
    }

    /// Sets the lobby metadata associated with the specified key from the
    /// specified lobby. //Selfmade
    pub fn set_lobby_data(&self, lobby: LobbyId, key: &str, value: &str) -> bool{
        let key = CString::new(key).unwrap();
        let value = CString::new(value).unwrap();
        unsafe {
            sys::SteamAPI_ISteamMatchmaking_SetLobbyData(self.mm, lobby.0, key.as_ptr(), value.as_ptr())
        }
    }

    pub fn lobby_member_data(&self, lobby: LobbyId, member: SteamId, key: &str) -> Option<&str> {
        let key = CString::new(key).unwrap();
        let data = unsafe {
            let data = sys::SteamAPI_ISteamMatchmaking_GetLobbyMemberData(self.mm, lobby.0, member.0, key.as_ptr());
            let data = CStr::from_ptr(data);

            data
        };

        let data = data.to_str().unwrap();

        match data.is_empty() {
            false => Some(data),
            true => None,
        }
    }

    pub fn set_lobby_member_data(&self, lobby: LobbyId, key: &str, value: &str){
        let key = CString::new(key).unwrap();
        let value = CString::new(value).unwrap();
        unsafe {
            sys::SteamAPI_ISteamMatchmaking_SetLobbyMemberData(self.mm, lobby.0, key.as_ptr(), value.as_ptr())
        }
    }

    /// Exits the passed lobby
    pub fn leave_lobby(&self, lobby: LobbyId) {
        unsafe {
            sys::SteamAPI_ISteamMatchmaking_LeaveLobby(self.mm, lobby.0);
        }
    }

    /// Returns the current limit on the number of players in a lobby.
    ///
    /// Returns `[None]` if no metadata is available for the specified lobby.
    pub fn lobby_member_limit(&self, lobby: LobbyId) -> Option<usize> {
        unsafe {
            let count = sys::SteamAPI_ISteamMatchmaking_GetLobbyMemberLimit(self.mm, lobby.0);
            match count {
                0 => None,
                _ => Some(count as usize),
            }
        }
    }

    /// Returns the steam id of the current owner of the passed lobby
    pub fn lobby_owner(&self, lobby: LobbyId) -> SteamId {
        unsafe {
            SteamId(sys::SteamAPI_ISteamMatchmaking_GetLobbyOwner(
                self.mm, lobby.0,
            ))
        }
    }

    /// Returns the number of players in a lobby.
    ///
    /// Useful if you are not currently in the lobby
    pub fn lobby_member_count(&self, lobby: LobbyId) -> usize {
        unsafe {
            let count = sys::SteamAPI_ISteamMatchmaking_GetNumLobbyMembers(self.mm, lobby.0);
            count as usize
        }
    }

    /// Returns a list of members currently in the lobby
    pub fn lobby_members(&self, lobby: LobbyId) -> Vec<SteamId> {
        unsafe {
            let count = sys::SteamAPI_ISteamMatchmaking_GetNumLobbyMembers(self.mm, lobby.0);
            let mut members = Vec::with_capacity(count as usize);
            for idx in 0..count {
                members.push(SteamId(
                    sys::SteamAPI_ISteamMatchmaking_GetLobbyMemberByIndex(self.mm, lobby.0, idx),
                ))
            }
            members
        }
    }

    /// Sets whether or not a lobby is joinable by other players. This always defaults to enabled
    /// for a new lobby.
    ///
    /// If joining is disabled, then no players can join, even if they are a friend or have been
    /// invited.
    ///
    /// Lobbies with joining disabled will not be returned from a lobby search.
    ///
    /// Returns true on success, false if the current user doesn't own the lobby.
    pub fn set_lobby_joinable(&self, lobby: LobbyId, joinable: bool) -> bool {
        unsafe { sys::SteamAPI_ISteamMatchmaking_SetLobbyJoinable(self.mm, lobby.0, joinable) }
    }
}

/// Flags describing how a users lobby state has changed. This is provided from `LobbyChatUpdate`.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ChatMemberStateChange {
    /// This user has joined or is joining the lobby.
    Entered,

    /// This user has left or is leaving the lobby.
    Left,

    /// User disconnected without leaving the lobby first.
    Disconnected,

    /// The user has been kicked.
    Kicked,

    /// The user has been kicked and banned.
    Banned,
}

/// A lobby chat room state has changed, this is usually sent when a user has joined or left the lobby.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LobbyChatUpdate {
    /// The Steam ID of the lobby.
    pub lobby: LobbyId,
    /// The user who's status in the lobby just changed - can be recipient.
    pub user_changed: SteamId,
    /// Chat member who made the change. This can be different from `user_changed` if kicking, muting, etc. For example, if one user kicks another from the lobby, this will be set to the id of the user who initiated the kick.
    pub making_change: SteamId,

    /// "ChatMemberStateChange" values.
    pub member_state_change: ChatMemberStateChange,
}

unsafe impl Callback for LobbyChatUpdate {
    const ID: i32 = 506;
    const SIZE: i32 = ::std::mem::size_of::<sys::LobbyChatUpdate_t>() as i32;

    unsafe fn from_raw(raw: *mut c_void) -> Self {
        let val = &mut *(raw as *mut sys::LobbyChatUpdate_t);

        LobbyChatUpdate {
            lobby: LobbyId(val.m_ulSteamIDLobby),
            user_changed: SteamId(val.m_ulSteamIDUserChanged),
            making_change: SteamId(val.m_ulSteamIDUserChanged),
            member_state_change: match val.m_rgfChatMemberStateChange {
                x if x == sys::EChatMemberStateChange::k_EChatMemberStateChangeEntered as u32 => {
                    ChatMemberStateChange::Entered
                }
                x if x == sys::EChatMemberStateChange::k_EChatMemberStateChangeLeft as u32 => {
                    ChatMemberStateChange::Left
                }
                x if x
                    == sys::EChatMemberStateChange::k_EChatMemberStateChangeDisconnected as u32 =>
                {
                    ChatMemberStateChange::Disconnected
                }
                x if x == sys::EChatMemberStateChange::k_EChatMemberStateChangeKicked as u32 => {
                    ChatMemberStateChange::Kicked
                }
                x if x == sys::EChatMemberStateChange::k_EChatMemberStateChangeBanned as u32 => {
                    ChatMemberStateChange::Banned
                }
                _ => unreachable!(),
            },
        }
    }
}

#[test]
#[serial]
fn test_lobby() {
    let (client, single) = Client::init().unwrap();
    let mm = client.matchmaking();

    mm.request_lobby_list(|v| {
        println!("List: {:?}", v);
    });
    mm.create_lobby(LobbyType::Private, 4, |v| {
        println!("Create: {:?}", v);
    });

    for _ in 0..100 {
        single.run_callbacks();
        ::std::thread::sleep(::std::time::Duration::from_millis(100));
    }
}
