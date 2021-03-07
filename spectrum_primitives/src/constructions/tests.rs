use super::{MultiKeyVdpf, TwoKeyVdpf};

mod two_key_vdpf_with_jubjub {
    use super::*;
    check_vdpf!(TwoKeyVdpf);
}

mod many_key_vdpf_with_jubjub {
    use super::*;
    check_vdpf!(MultiKeyVdpf);
}
