use idiomatic_core::pack::LoadedPack;
use idiomatic_core::render::render_skill;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn renders_python_skill_from_seed_pack() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();

    let skill = render_skill(&set, "python");

    // frontmatter
    assert!(skill.starts_with("---\n"));
    assert!(skill.contains("name: idiomatic-python"));
    // teaches each idiom (titles from the seed pack)
    assert!(skill.contains("Use `is None`"));
    assert!(skill.contains("Flatten deep nesting")); // skill-only idiom included
    // a fenced python example block is rendered
    assert!(skill.contains("```python"));
    assert!(skill.contains("# Avoid:"));
    assert!(skill.contains("# Prefer:"));
}
