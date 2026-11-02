pub fn allows(
    mode: u16,
    owner_uid: u32,
    owner_gid: u32,
    uid: u32,
    gid: u32,
    requested: u16,
) -> bool {
    if uid == 0 {
        return true;
    }
    let shift = if uid == owner_uid {
        6
    } else if gid == owner_gid {
        3
    } else {
        0
    };
    ((mode >> shift) & requested) == requested
}

#[cfg(test)]
mod tests {
    use super::allows;

    #[test]
    fn selects_owner_group_and_other_bits_and_root_bypasses_them() {
        let mode = 0o640;
        assert!(allows(mode, 1000, 1000, 1000, 1000, 6));
        assert!(allows(mode, 1000, 1000, 2000, 1000, 4));
        assert!(!allows(mode, 1000, 1000, 2000, 2000, 4));
        assert!(allows(mode, 1000, 1000, 0, 0, 7));
    }

    #[test]
    fn requires_all_requested_access_bits() {
        assert!(!allows(0o744, 1000, 1000, 2000, 2000, 3));
        assert!(allows(0o777, 1000, 1000, 2000, 2000, 7));
    }
}
