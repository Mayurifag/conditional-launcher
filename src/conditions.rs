use crate::os::OsOperations;
use serde::{Deserialize, Serialize};
use sysinfo::Disks;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Conditions {
    pub internet: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_mounted: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keepassxc_unlocked: bool,
}

pub struct ConditionStatus {
    pub internet_ok: bool,
    pub partition_ok: bool,
    pub keepassxc_ok: bool,
}

impl ConditionStatus {
    pub fn ready(&self) -> bool {
        self.internet_ok && self.partition_ok && self.keepassxc_ok
    }
}

impl Conditions {
    pub fn requires_management(&self) -> bool {
        self.internet || self.partition_mounted.is_some() || self.keepassxc_unlocked
    }

    pub fn check(
        &self,
        os_ops: &dyn OsOperations,
        has_internet: bool,
        disks: &Disks,
        keepassxc_unlocked: bool,
    ) -> ConditionStatus {
        ConditionStatus {
            internet_ok: !self.internet || has_internet,
            partition_ok: self
                .partition_mounted
                .as_deref()
                .is_none_or(|path| os_ops.is_partition_mounted(path, disks)),
            keepassxc_ok: !self.keepassxc_unlocked || keepassxc_unlocked,
        }
    }
}
