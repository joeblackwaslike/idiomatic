use idiomatic_core::pack::LoadedPack;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn seed_pack_resolves_to_four_idioms() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();
    assert_eq!(set.len(), 4);

    let ids: Vec<&str> = set.iter().map(|i| i.id.as_str()).collect();
    insta::assert_yaml_snapshot!(ids);
}
