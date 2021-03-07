mod two_key {
    use spectrum_primitives::TwoKeyVdpf;
    check_protocol!(TwoKeyVdpf);
}

mod multi_key {
    use spectrum_primitives::MultiKeyVdpf;
    check_protocol!(MultiKeyVdpf);
}
