//! What kind of failure a GraphQL error is, as a `code` extension, so a frontend can act on it
//! without reading the message (#224).

use async_graphql::{Error, ErrorExtensions};

/// The kinds of failure a portal tells its frontend apart. The message stays for people; the
/// code is what the frontend branches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// No signed-in viewer.
    Unauthenticated,
    /// Policy denied it.
    Forbidden,
    /// Policy allows it after a challenge; the error's `challengeQid` names the one opened.
    ChallengeRequired,
    /// The request itself is wrong; sending it again won't help.
    InvalidInput,
    /// What it names doesn't exist, or the viewer can't see that it does.
    NotFound,
    /// A service awzy needs didn't answer; trying again later may work.
    Unavailable,
}

impl ErrorCode {
    /// The code as the `code` extension carries it.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Unauthenticated => "UNAUTHENTICATED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::ChallengeRequired => "CHALLENGE_REQUIRED",
            ErrorCode::InvalidInput => "INVALID_INPUT",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::Unavailable => "UNAVAILABLE",
        }
    }

    /// A GraphQL error with this code and `message`.
    pub fn error(self, message: impl Into<String>) -> Error {
        Error::new(message).extend_with(|_, e| e.set("code", self.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::Value;

    use super::*;

    #[test]
    fn an_error_carries_its_code_as_an_extension() {
        let e = ErrorCode::ChallengeRequired.error("touch your key");
        assert_eq!(e.message, "touch your key");
        let code = e.extensions.as_ref().and_then(|x| x.get("code")).cloned();
        assert_eq!(code, Some(Value::from("CHALLENGE_REQUIRED")));
    }
}
