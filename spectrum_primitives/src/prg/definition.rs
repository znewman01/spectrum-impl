pub trait PRG {
    type Seed;
    type Output;

    fn new_seed() -> Self::Seed;
    fn output_size(&self) -> usize;
    fn eval(&self, seed: &Self::Seed) -> Self::Output;
    fn null_output(&self) -> Self::Output;
}

#[cfg(any(test, feature = "testing"))]
macro_rules! check_prg {
    ($type:ty,$mod_name:ident) => {
        mod $mod_name {
            #![allow(unused_imports)]
            use super::*;
            use proptest::prelude::*;
            use std::collections::HashSet;
            use std::iter::repeat_with;

            #[test]
            fn check_bounds() {
                fn check<P: PRG>() {}
                check::<$type>();
            }

            proptest! {
                /// new_seed should give random seeds.
                #[test]
                fn test_seed_random(prg: $type) {
                    prop_assume!(prg.output_size() > 0);
                    let results: HashSet<_> = repeat_with(<$type as PRG>::new_seed)
                        .take(5)
                        .map(|s| prg.eval(&s))
                        .collect();
                    prop_assert!(results.len() > 1);
                }

                #[test]
                /// Evaluation with the same seed should give the same result.
                fn test_eval_deterministic(prg: $type, seed: <$type as PRG>::Seed) {
                    prop_assert_eq!(prg.eval(&seed), prg.eval(&seed));
                }

                /// Evaluation with different seeds should give different results.
                #[test]
                fn test_eval_pseudorandom(prg: $type, seeds in proptest::collection::hash_set(any::<<$type as PRG>::Seed>(), 0..5)) {
                    prop_assume!(prg.output_size() > 0);
                    use std::collections::HashSet;
                    let results: HashSet<_> = seeds.iter().map(|s| prg.eval(s)).collect();
                    prop_assert_eq!(results.len(), seeds.len());
                }
            }
        }
    };
    ($type:ty) => {
        check_prg!($type, prg);
    };
}
