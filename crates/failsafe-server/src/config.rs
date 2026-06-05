use std::net::SocketAddr;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, Clone, Default)]
pub struct ListenConfig {
    pub listen: Option<SocketAddr>,
    pub host: Option<String>,
    pub port: Option<u16>,
}

impl ListenConfig {
    pub fn resolve(&self) -> Result<SocketAddr, String> {
        if let Some(addr) = self.listen {
            return Ok(addr);
        }

        if self.host.is_some() || self.port.is_some() {
            let host = self
                .host
                .clone()
                .or_else(|| std::env::var("FAILSAFE_LISTEN_HOST").ok())
                .unwrap_or_else(|| DEFAULT_HOST.to_owned());
            let port = self.port.unwrap_or_else(|| {
                parse_port_env("FAILSAFE_LISTEN_PORT")
                    .ok()
                    .flatten()
                    .unwrap_or(DEFAULT_PORT)
            });

            return parse_socket_addr(&host, port);
        }

        if let Ok(listen) = std::env::var("FAILSAFE_LISTEN") {
            return listen
                .parse()
                .map_err(|error| format!("invalid FAILSAFE_LISTEN: {error}"));
        }

        let host = std::env::var("FAILSAFE_LISTEN_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_owned());
        let port = parse_port_env("FAILSAFE_LISTEN_PORT")?.unwrap_or(DEFAULT_PORT);

        parse_socket_addr(&host, port)
    }
}

fn parse_port_env(name: &str) -> Result<Option<u16>, String> {
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map(Some)
            .map_err(|error| format!("invalid {name}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("invalid {name}: not unicode")),
    }
}

fn parse_socket_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    format!("{host}:{port}")
        .parse()
        .map_err(|error| format!("invalid listen address {host}:{port}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_explicit_host_and_port() {
        let addr = ListenConfig {
            host: Some("0.0.0.0".to_owned()),
            port: Some(9000),
            ..Default::default()
        }
        .resolve()
        .unwrap();
        assert_eq!(addr, "0.0.0.0:9000".parse().unwrap());
    }

    #[test]
    fn listen_flag_takes_precedence() {
        let addr = ListenConfig {
            listen: Some("192.168.1.1:3000".parse().unwrap()),
            host: Some("0.0.0.0".to_owned()),
            port: Some(9000),
        }
        .resolve()
        .unwrap();
        assert_eq!(addr, "192.168.1.1:3000".parse().unwrap());
    }

    #[test]
    fn host_defaults_port_when_only_host_set() {
        let addr = ListenConfig {
            host: Some("0.0.0.0".to_owned()),
            ..Default::default()
        }
        .resolve()
        .unwrap();
        assert_eq!(addr, "0.0.0.0:8080".parse().unwrap());
    }
}
