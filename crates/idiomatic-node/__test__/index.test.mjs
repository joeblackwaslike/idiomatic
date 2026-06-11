import test from 'node:test';
import assert from 'node:assert/strict';
import { lint, autofix, renderSkill } from '../index.js';

test('lint reports compare-none', () => {
  const hits = lint('if x == None:\n    pass\n', 'python');
  assert.ok(hits.some((h) => h.id === 'compare-none'));
  const h = hits.find((h) => h.id === 'compare-none');
  assert.ok(h.start < h.end);
});

test('autofix rewrites in place', () => {
  const { fixed, count } = autofix('if x == None:\n    pass\n', 'python');
  assert.equal(fixed, 'if x is None:\n    pass\n');
  assert.equal(count, 1);
});

test('autofix leaves good code untouched', () => {
  const { fixed, count } = autofix('if x is None:\n    pass\n', 'python');
  assert.equal(count, 0);
  assert.equal(fixed, 'if x is None:\n    pass\n');
});

test('renderSkill python', () => {
  const skill = renderSkill('python');
  assert.ok(skill.includes('name: idiomatic-python'));
  assert.ok(skill.includes('Use `is None`'));
});

test('typescript supported', () => {
  const { fixed } = autofix('const x = a;\n', 'typescript');
  assert.equal(fixed, 'const x = a;\n');
  assert.ok(renderSkill('typescript').includes('name: idiomatic-typescript'));
});

test('unknown language throws', () => {
  assert.throws(() => lint('x', 'cobol'));
  assert.throws(() => renderSkill('cobol'));
});
