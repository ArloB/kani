#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionParseError(pub String);

impl std::error::Error for PermissionParseError {}

impl std::fmt::Display for PermissionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait AuthRequirement {
    fn required_permission() -> Option<Permission>;
}

pub struct IsAuthenticated;
impl AuthRequirement for IsAuthenticated {
    fn required_permission() -> Option<Permission> {
        None 
    }
}

macro_rules! permissions {
    (
        $(
            $resource:ident => $resource_str:literal {
                $( $action:ident => $action_str:literal ),* $(,)?
            }
        ),* $(,)?
    ) => {
        $(
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum $resource {
                $( $action ),*
            }

            impl std::fmt::Display for $resource {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let s = match self {
                        $( Self::$action => $action_str ),*
                    };
                    write!(f, "{}", s)
                }
            }

            impl std::str::FromStr for $resource {
                type Err = PermissionParseError;
                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    match s {
                        $( $action_str => Ok(Self::$action), )*
                        _ => Err(PermissionParseError(format!("unknown {} action: {}", $resource_str, s))),
                    }
                }
            }
        )*

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Permission {
            $( $resource($resource) ),*
        }

        impl std::fmt::Display for Permission {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( Permission::$resource(a) => write!(f, "{}:{}", $resource_str, a) ),*
                }
            }
        }

        impl std::str::FromStr for Permission {
            type Err = PermissionParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (resource, action) = s
                    .split_once(':')
                    .ok_or_else(|| PermissionParseError(format!("invalid permission format: {s}")))?;
                match resource {
                    $( $resource_str => Ok(Permission::$resource(action.parse()?)), )*
                    _ => Err(PermissionParseError(format!("unknown resource: {resource}"))),
                }
            }
        }

        pub mod guards {
            pub struct Authenticated;
            impl super::AuthRequirement for Authenticated {
                fn required_permission() -> Option<super::Permission> { None }
            }
        
            $(
                $(
                    ::paste::paste! {
                        pub struct [<$resource $action>];
        
                        impl super::AuthRequirement for [<$resource $action>] {
                            fn required_permission() -> Option<super::Permission> {
                                Some(super::Permission::$resource(
                                    super::$resource::$action
                                ))
                            }
                        }
                    }
                )*
            )*
        }
    };
}

permissions! {
    Library => "library" {
        View   => "view",
        Add    => "add",
        Delete => "delete",
        Refresh => "refresh",
        Manage => "manage",
    },
    Chapter => "chapter" {
        Download => "download",
        Delete   => "delete",
    },
    Source => "source" {
        Browse         => "browse",
        Install        => "install",
        Delete         => "delete",
        ToggleEnabled  => "toggle_enabled",
        Configure      => "configure",
    },
    Settings => "settings" {
        View         => "view",
        EditDownload => "edit_download",
        EditScan     => "edit_scan",
        EditAdvanced => "edit_advanced",
    },
    User => "user" {
        Manage => "manage",
    },
}

impl serde::Serialize for Permission {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Permission {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
