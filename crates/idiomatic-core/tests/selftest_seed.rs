use idiomatic_core::pack::LoadedPack;
use idiomatic_core::selftest::run_selftests;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn every_seed_idiom_passes_its_own_examples() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();

    let results = run_selftests(&set);
    let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
    assert!(failures.is_empty(), "self-test failures: {failures:#?}");
}
