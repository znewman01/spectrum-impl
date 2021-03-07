use crate::prg::Prg;
/// Seed homomorphic PRG
///
/// The seeds should form a group, and the outputs a (roughly) isomorphic group.
pub trait SeedHomomorphicPrg: Prg {
    fn combine_seeds(&self, seeds: Vec<Self::Seed>) -> Self::Seed;
    fn combine_outputs(&self, outputs: &[&Self::Output]) -> Self::Output;
    fn null_seed() -> Self::Seed;
}

#[cfg(test)]
macro_rules! check_seed_homomorphic_prg {
    ($type:ty) => {
        mod shprg {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            use crate::prg::{Prg, SeedHomomorphicPrg};

            #[test]
            fn check_bounds() {
                fn check<P: SeedHomomorphicPrg>() {}
                check::<$type>();
            }

            proptest! {
                /// The null seed should be the identity for seed group operations.
                #[test]
                fn test_null_seed_identity(prg: $type, seed: <$type as Prg>::Seed) {
                    assert_eq!(
                        seed.clone() + <$type as SeedHomomorphicPrg>::null_seed(),
                        seed
                    );
                }

                /// Verify that the "null seed" is the identity after expansion.
                #[test]
                fn test_null_seed_maps_to_null_output(prg: $type) {
                    assert_eq!(prg.eval(&<$type as SeedHomomorphicPrg>::null_seed()), prg.null_output());
                }

                /// The null output should be the identity for combining outputs.
                #[test]
                fn test_null_output_identity(prg: $type, seed: <$type as Prg>::Seed) {
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
                fn test_evaluation_preserves_operation(prg: $type, seed1: <$type as Prg>::Seed, seed2: <$type as Prg>::Seed) {
                    assert_eq!(
                        prg.combine_outputs(&[&prg.eval(&seed1), &prg.eval(&seed2)]),
                        prg.eval(&prg.combine_seeds(vec![seed1, seed2]))
                    );
                }
            }
        }
    };
}
