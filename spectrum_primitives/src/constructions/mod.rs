mod aes_prg;

#[cfg(any(test, feature = "testing"))]
macro_rules! check_construction {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            use super::*;
            check_construction!($type);
        }
    };
    ($type:ty) => {
        #[allow(unused_imports)]
        use crate::{
            lss::{LinearlyShareable, Shareable},
            prg::{GroupPRG, GroupPrgSeed, SeedHomomorphicPRG, PRG},
        };
        check_group_laws!($type);
        check_field_laws!($type);
        check_sampleable!($type);
        check_shareable!($type);
        check_linearly_shareable!($type);
        check_prg!(GroupPRG<$type>);
        check_seed_homomorphic_prg!(GroupPRG<$type>);
        check_group_laws!(GroupPrgSeed<$type>, prg_seed_group_laws);
    };
}

mod baby;
mod jubjub;
