use nvim_oxi::{self as oxi, lua, Object};
use oxi::conversion::{self, FromObject};
use oxi::serde::Deserializer;
use serde::{Deserialize, Serialize};

/// Config is used to store the configuration of the plugin
#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    /// Port of the RPC server to connect to
    #[serde(default = "default_rpc_port")]
    pub(crate) rpc_port: u16,
}

/// Default RPC port
fn default_rpc_port() -> u16 {
    50051
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_port: default_rpc_port(),
        }
    }
}

impl FromObject for Config {
    fn from_object(obj: Object) -> Result<Self, conversion::Error> {
        Self::deserialize(Deserializer::new(obj)).map_err(Into::into)
    }
}

impl lua::Poppable for Config {
    unsafe fn pop(lstate: *mut lua::ffi::lua_State) -> Result<Self, lua::Error> {
        let Ok(obj) = Object::pop(lstate) else {
            return Ok(Config::default());
        };

        Self::from_object(obj).map_err(lua::Error::pop_error_from_err::<Self, _>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.rpc_port, 50051);
    }
}
