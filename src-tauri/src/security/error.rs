/// Messages never carry paths or hosts.
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("network is disabled by Air-Gap mode")]
    AirGapEnabled,
    #[error("host is not allowed for this purpose")]
    HostNotAllowed,
    #[error("path is outside the registered folder")]
    OutsideScope,
    #[error("path could not be resolved")]
    Unresolvable(#[source] std::io::Error),
}
