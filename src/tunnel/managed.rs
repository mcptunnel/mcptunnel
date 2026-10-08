//! Future hosted-service contract. This is an unavailable integration, not a billing or
//! provisioning implementation. The service, not this library, must verify paid entitlement.
use std::future::Future;

/// A bearer credential for the future hosted service; never serialized or logged here.
pub struct ProvisionRequest {
    pub service_access_token: String,
    pub hostname: String,
}
pub struct ManagedLease {
    pub id: String,
    pub public_origin: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum ManagedError {
    Unavailable,
    Unauthenticated,
    NotEntitled,
    Revoked,
    HostnameNotOwned,
}

/// An actual backend must authenticate every operation, record hostname ownership and
/// recheck entitlement at renewal. A revoked/expired lease must stop routing.
pub trait ManagedProvisioner {
    fn provision(
        &self,
        request: ProvisionRequest,
    ) -> impl Future<Output = Result<ManagedLease, ManagedError>> + Send;
    fn renew(
        &self,
        service_access_token: &str,
        lease_id: &str,
    ) -> impl Future<Output = Result<ManagedLease, ManagedError>> + Send;
    fn revoke(
        &self,
        service_access_token: &str,
        lease_id: &str,
    ) -> impl Future<Output = Result<(), ManagedError>> + Send;
}
pub struct UnavailableManagedService;
impl ManagedProvisioner for UnavailableManagedService {
    async fn provision(&self, _: ProvisionRequest) -> Result<ManagedLease, ManagedError> {
        Err(ManagedError::Unavailable)
    }
    async fn renew(&self, _: &str, _: &str) -> Result<ManagedLease, ManagedError> {
        Err(ManagedError::Unavailable)
    }
    async fn revoke(&self, _: &str, _: &str) -> Result<(), ManagedError> {
        Err(ManagedError::Unavailable)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_backend_never_grants_a_hostname_or_lease() {
        let service = UnavailableManagedService;
        assert!(matches!(
            service
                .provision(ProvisionRequest {
                    service_access_token: "local-paid-flag".into(),
                    hostname: "stable.example".into()
                })
                .await,
            Err(ManagedError::Unavailable)
        ));
        assert!(matches!(
            service.renew("local-paid-flag", "lease").await,
            Err(ManagedError::Unavailable)
        ));
        assert_eq!(
            service.revoke("local-paid-flag", "lease").await,
            Err(ManagedError::Unavailable)
        );
    }
}
