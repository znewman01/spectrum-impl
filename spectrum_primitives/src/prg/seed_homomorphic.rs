use crate::prg::PRG;
/// Seed homomorphic PRG
///
/// The seeds should form a group, and the outputs a (roughly) isomorphic group.
pub trait SeedHomomorphicPRG: PRG {
    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed;
    fn combine_outputs(&self, outputs: &[&Self::Output]) -> Self::Output;
    fn null_seed(&self) -> Self::Seed;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_seed_homomorphic_prg {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;

            #[test]
            fn check_bounds() {
                fn check<P: SeedHomomorphicPRG>() {}
                check::<$type>();
            }

            proptest! {
                /// Verify that the "null seed" is the identity after expansion.
                #[test]
                fn test_null_seed_identity(prg: $type, seed: <$type as PRG>::Seed) {
                    // ensure combine(null, null) = null
                    assert_eq!(
                        prg.null_output(),
                        prg.combine_outputs(&[&prg.null_output(), &prg.null_output()])
                    );

                    // ensure combine(null, eval) = eval
                    assert_eq!(
                        prg.eval(&seed),
                        prg.combine_outputs(&[&prg.eval(&seed), &prg.null_output()])
                    );

                    // ensure combine(eval, null) = eval
                    assert_eq!(
                        prg.eval(&seed),
                        prg.combine_outputs(&[&prg.null_output(), &prg.eval(&seed)])
                    );
                }

                /// Verify G(x . y) == G(x) . G(y)
                #[test]
                fn test_evaluation_preserves_operation(prg: $type, seed1: <$type as PRG>::Seed, seed2: <$type as PRG>::Seed) {
                    assert_eq!(
                        prg.combine_outputs(&[&prg.eval(&seed1), &prg.eval(&seed2)]),
                        prg.eval(&prg.combine_seeds(vec![seed1, seed2]))
                    );
                }
            }
        }
    };
    ($type:ty) => {
        check_seed_homomorphic_prg!($type, seed_homomorphic_prg);
    };
}
