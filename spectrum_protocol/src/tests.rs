mod two_key {
    use crate::secure::Wrapper;
    use spectrum_primitives::TwoKeyVdpf;
    check_protocol!(Wrapper<TwoKeyVdpf>);
}

mod multi_key {
    use crate::secure::Wrapper;
    use spectrum_primitives::MultiKeyVdpf;
    check_protocol!(Wrapper<MultiKeyVdpf>);
}

mod two_key_pub {
    use crate::secure::Wrapper;
    use spectrum_primitives::TwoKeyPubVdpf;
    check_protocol!(Wrapper<TwoKeyPubVdpf>);
}
