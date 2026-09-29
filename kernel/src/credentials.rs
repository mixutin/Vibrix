//! Fixed-capacity Unix-style process credentials and discretionary access policy.
//!
//! This is the first M11 multi-user slice. It models real/effective/saved
//! user and group identities, a bounded supplementary group list, and the
//! traditional owner/group/other mode-bit access decision. It does not provide
//! authentication, a persistent account database, set-id syscalls, ACLs,
//! capabilities, or VFS enforcement yet.

pub const MAX_SUPPLEMENTARY_GROUPS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Uid(u32);

impl Uid {
    pub const ROOT: Self = Self(0);

    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Gid(u32);

impl Gid {
    pub const ROOT: Self = Self(0);

    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Regular,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
    Execute,
}

impl Access {
    const fn mode_bit(self) -> u16 {
        match self {
            Self::Read => 0b100,
            Self::Write => 0b010,
            Self::Execute => 0b001,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TooManyGroups,
    PermissionDenied,
    InvalidMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Credentials {
    pub real_uid: Uid,
    pub effective_uid: Uid,
    pub saved_uid: Uid,
    pub real_gid: Gid,
    pub effective_gid: Gid,
    pub saved_gid: Gid,
    supplementary: [Gid; MAX_SUPPLEMENTARY_GROUPS],
    supplementary_len: u8,
}

impl Credentials {
    pub const fn root() -> Self {
        Self {
            real_uid: Uid::ROOT,
            effective_uid: Uid::ROOT,
            saved_uid: Uid::ROOT,
            real_gid: Gid::ROOT,
            effective_gid: Gid::ROOT,
            saved_gid: Gid::ROOT,
            supplementary: [Gid::ROOT; MAX_SUPPLEMENTARY_GROUPS],
            supplementary_len: 0,
        }
    }

    pub const fn user(uid: Uid, gid: Gid) -> Self {
        Self {
            real_uid: uid,
            effective_uid: uid,
            saved_uid: uid,
            real_gid: gid,
            effective_gid: gid,
            saved_gid: gid,
            supplementary: [Gid::ROOT; MAX_SUPPLEMENTARY_GROUPS],
            supplementary_len: 0,
        }
    }

    pub fn with_supplementary_groups(uid: Uid, gid: Gid, groups: &[Gid]) -> Result<Self, Error> {
        if groups.len() > MAX_SUPPLEMENTARY_GROUPS {
            return Err(Error::TooManyGroups);
        }
        let mut credentials = Self::user(uid, gid);
        credentials.supplementary[..groups.len()].copy_from_slice(groups);
        credentials.supplementary_len = groups.len() as u8;
        Ok(credentials)
    }

    pub fn supplementary_groups(&self) -> &[Gid] {
        &self.supplementary[..usize::from(self.supplementary_len)]
    }

    /// Replace this process's supplementary group list.
    ///
    /// Early Vibrix follows a deliberately narrow policy: only a process with
    /// effective UID 0 may mutate its group access list. Capacity is checked
    /// before mutation so failure leaves the credential object unchanged.
    pub fn set_supplementary_groups(&mut self, groups: &[Gid]) -> Result<(), Error> {
        if !self.is_superuser() {
            return Err(Error::PermissionDenied);
        }
        if groups.len() > MAX_SUPPLEMENTARY_GROUPS {
            return Err(Error::TooManyGroups);
        }

        self.supplementary = [Gid::ROOT; MAX_SUPPLEMENTARY_GROUPS];
        self.supplementary[..groups.len()].copy_from_slice(groups);
        self.supplementary_len = groups.len() as u8;
        Ok(())
    }

    /// Atomically update real/effective/saved user IDs.
    ///
    /// Effective UID 0 may select arbitrary IDs. Otherwise every requested ID
    /// must already be one of this process's real/effective/saved IDs. A denied
    /// request leaves the entire credential tuple unchanged.
    pub fn set_res_uids(
        &mut self,
        real: Option<Uid>,
        effective: Option<Uid>,
        saved: Option<Uid>,
    ) -> Result<(), Error> {
        let privileged = self.is_superuser();
        let allowed = [self.real_uid, self.effective_uid, self.saved_uid];
        if !privileged
            && [real, effective, saved]
                .into_iter()
                .flatten()
                .any(|uid| !allowed.contains(&uid))
        {
            return Err(Error::PermissionDenied);
        }
        if let Some(uid) = real {
            self.real_uid = uid;
        }
        if let Some(uid) = effective {
            self.effective_uid = uid;
        }
        if let Some(uid) = saved {
            self.saved_uid = uid;
        }
        Ok(())
    }

    /// Atomically update real/effective/saved group IDs.
    ///
    /// Group-ID privilege is governed by effective UID 0. Non-root callers may
    /// only select IDs already present in their real/effective/saved GID tuple.
    pub fn set_res_gids(
        &mut self,
        real: Option<Gid>,
        effective: Option<Gid>,
        saved: Option<Gid>,
    ) -> Result<(), Error> {
        let privileged = self.is_superuser();
        let allowed = [self.real_gid, self.effective_gid, self.saved_gid];
        if !privileged
            && [real, effective, saved]
                .into_iter()
                .flatten()
                .any(|gid| !allowed.contains(&gid))
        {
            return Err(Error::PermissionDenied);
        }
        if let Some(gid) = real {
            self.real_gid = gid;
        }
        if let Some(gid) = effective {
            self.effective_gid = gid;
        }
        if let Some(gid) = saved {
            self.saved_gid = gid;
        }
        Ok(())
    }

    pub fn is_superuser(&self) -> bool {
        self.effective_uid == Uid::ROOT
    }

    pub fn in_group(&self, gid: Gid) -> bool {
        self.effective_gid == gid || self.supplementary_groups().contains(&gid)
    }

    pub fn permits(
        &self,
        owner: Uid,
        group: Gid,
        mode: u16,
        kind: ObjectKind,
        access: Access,
    ) -> Result<bool, Error> {
        if mode & !0o7777 != 0 {
            return Err(Error::InvalidMode);
        }

        if self.is_superuser() {
            if access == Access::Execute && kind == ObjectKind::Regular {
                return Ok(mode & 0o111 != 0);
            }
            return Ok(true);
        }

        let class = if self.effective_uid == owner {
            (mode >> 6) & 0o7
        } else if self.in_group(group) {
            (mode >> 3) & 0o7
        } else {
            mode & 0o7
        };

        Ok(class & access.mode_bit() != 0)
    }
}

pub fn self_test() -> Result<(), Error> {
    let owner = Uid::from_raw(1000);
    let group = Gid::from_raw(100);
    let user =
        Credentials::with_supplementary_groups(Uid::from_raw(2000), Gid::from_raw(200), &[group])?;

    if !user.permits(owner, group, 0o0640, ObjectKind::Regular, Access::Read)? {
        return Err(Error::InvalidMode);
    }
    if user.permits(owner, group, 0o0640, ObjectKind::Regular, Access::Write)? {
        return Err(Error::InvalidMode);
    }

    let mut root = Credentials::root();
    root.set_supplementary_groups(&[Gid::from_raw(42), Gid::from_raw(43)])?;
    if root.supplementary_groups() != [Gid::from_raw(42), Gid::from_raw(43)] {
        return Err(Error::InvalidMode);
    }
    if !root.permits(owner, group, 0, ObjectKind::Directory, Access::Execute)? {
        return Err(Error::InvalidMode);
    }
    if root.permits(owner, group, 0, ObjectKind::Regular, Access::Execute)? {
        return Err(Error::InvalidMode);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constructors_preserve_real_effective_and_saved_ids() {
        let credentials = Credentials::user(Uid::from_raw(1000), Gid::from_raw(100));
        assert_eq!(credentials.real_uid.get(), 1000);
        assert_eq!(credentials.effective_uid.get(), 1000);
        assert_eq!(credentials.saved_uid.get(), 1000);
        assert_eq!(credentials.real_gid.get(), 100);
        assert_eq!(credentials.effective_gid.get(), 100);
        assert_eq!(credentials.saved_gid.get(), 100);
        assert!(!credentials.is_superuser());

        let root = Credentials::root();
        assert!(root.is_superuser());
        assert_eq!(root.effective_uid, Uid::ROOT);
        assert_eq!(root.effective_gid, Gid::ROOT);
    }

    #[test]
    fn supplementary_group_membership_is_bounded() {
        let groups = [
            Gid::from_raw(10),
            Gid::from_raw(11),
            Gid::from_raw(12),
            Gid::from_raw(13),
            Gid::from_raw(14),
            Gid::from_raw(15),
            Gid::from_raw(16),
            Gid::from_raw(17),
        ];
        let credentials =
            Credentials::with_supplementary_groups(Uid::from_raw(1000), Gid::from_raw(20), &groups)
                .unwrap();
        assert_eq!(credentials.supplementary_groups(), groups.as_slice());
        assert!(credentials.in_group(Gid::from_raw(14)));
        assert!(credentials.in_group(Gid::from_raw(20)));

        let too_many = [
            Gid::from_raw(1),
            Gid::from_raw(2),
            Gid::from_raw(3),
            Gid::from_raw(4),
            Gid::from_raw(5),
            Gid::from_raw(6),
            Gid::from_raw(7),
            Gid::from_raw(8),
            Gid::from_raw(9),
        ];
        assert_eq!(
            Credentials::with_supplementary_groups(
                Uid::from_raw(1000),
                Gid::from_raw(20),
                &too_many,
            ),
            Err(Error::TooManyGroups)
        );
    }

    #[test]
    fn supplementary_group_mutation_is_root_only_and_transactional() {
        let mut root = Credentials::root();
        let initial = [Gid::from_raw(10), Gid::from_raw(11)];
        root.set_supplementary_groups(&initial).unwrap();
        assert_eq!(root.supplementary_groups(), initial.as_slice());

        let too_many = [Gid::from_raw(99); MAX_SUPPLEMENTARY_GROUPS + 1];
        assert_eq!(
            root.set_supplementary_groups(&too_many),
            Err(Error::TooManyGroups)
        );
        assert_eq!(root.supplementary_groups(), initial.as_slice());

        let mut user = Credentials::user(Uid::from_raw(1000), Gid::from_raw(100));
        assert_eq!(
            user.set_supplementary_groups(&[Gid::from_raw(200)]),
            Err(Error::PermissionDenied)
        );
        assert!(user.supplementary_groups().is_empty());
    }

    #[test]
    fn root_and_nonroot_identity_transitions_are_atomic() {
        let mut credentials = Credentials::root();
        credentials
            .set_res_gids(
                Some(Gid::from_raw(10)),
                Some(Gid::from_raw(20)),
                Some(Gid::from_raw(30)),
            )
            .unwrap();
        credentials
            .set_res_uids(
                Some(Uid::from_raw(1000)),
                Some(Uid::from_raw(2000)),
                Some(Uid::from_raw(3000)),
            )
            .unwrap();
        assert_eq!(credentials.real_uid.get(), 1000);
        assert_eq!(credentials.effective_uid.get(), 2000);
        assert_eq!(credentials.saved_uid.get(), 3000);
        assert_eq!(credentials.real_gid.get(), 10);
        assert_eq!(credentials.effective_gid.get(), 20);
        assert_eq!(credentials.saved_gid.get(), 30);

        credentials
            .set_res_uids(None, Some(Uid::from_raw(3000)), None)
            .unwrap();
        assert_eq!(credentials.effective_uid.get(), 3000);

        let before = credentials;
        assert_eq!(
            credentials.set_res_uids(None, Some(Uid::from_raw(9999)), None),
            Err(Error::PermissionDenied)
        );
        assert_eq!(credentials, before);
        assert_eq!(
            credentials.set_res_gids(None, Some(Gid::from_raw(9999)), None),
            Err(Error::PermissionDenied)
        );
        assert_eq!(credentials, before);
    }

    #[test]
    fn owner_group_and_other_classes_are_selected_exclusively() {
        let owner = Uid::from_raw(1000);
        let group = Gid::from_raw(100);
        let owner_credentials = Credentials::user(owner, Gid::from_raw(999));
        assert!(
            owner_credentials
                .permits(owner, group, 0o0400, ObjectKind::Regular, Access::Read)
                .unwrap()
        );
        assert!(
            !owner_credentials
                .permits(owner, group, 0o0040, ObjectKind::Regular, Access::Read)
                .unwrap()
        );

        let group_credentials = Credentials::with_supplementary_groups(
            Uid::from_raw(2000),
            Gid::from_raw(200),
            &[group],
        )
        .unwrap();
        assert!(
            group_credentials
                .permits(owner, group, 0o0040, ObjectKind::Regular, Access::Read)
                .unwrap()
        );
        assert!(
            !group_credentials
                .permits(owner, group, 0o0004, ObjectKind::Regular, Access::Read)
                .unwrap()
        );

        let other_credentials = Credentials::user(Uid::from_raw(2000), Gid::from_raw(200));
        assert!(
            other_credentials
                .permits(owner, group, 0o0004, ObjectKind::Regular, Access::Read)
                .unwrap()
        );
    }

    #[test]
    fn superuser_bypasses_dac_except_regular_file_execute_without_x_bits() {
        let root = Credentials::root();
        let owner = Uid::from_raw(1000);
        let group = Gid::from_raw(100);

        assert!(
            root.permits(owner, group, 0, ObjectKind::Regular, Access::Read)
                .unwrap()
        );
        assert!(
            root.permits(owner, group, 0, ObjectKind::Regular, Access::Write)
                .unwrap()
        );
        assert!(
            root.permits(owner, group, 0, ObjectKind::Directory, Access::Execute)
                .unwrap()
        );
        assert!(
            !root
                .permits(owner, group, 0o0644, ObjectKind::Regular, Access::Execute)
                .unwrap()
        );
        assert!(
            root.permits(owner, group, 0o0100, ObjectKind::Regular, Access::Execute)
                .unwrap()
        );
    }

    #[test]
    fn invalid_mode_bits_fail_closed() {
        let credentials = Credentials::user(Uid::from_raw(1000), Gid::from_raw(100));
        assert_eq!(
            credentials.permits(
                Uid::from_raw(1000),
                Gid::from_raw(100),
                0x8000,
                ObjectKind::Regular,
                Access::Read,
            ),
            Err(Error::InvalidMode)
        );
    }

    #[test]
    fn production_self_test_passes() {
        self_test().unwrap();
    }
}
