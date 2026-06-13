import { describe, it, expect } from 'vitest';
import { applySearchReplace } from './searchReplace.js';

describe('searchReplace', () => {
  const sample = `function greet() {
  console.log("hello");
  return 42;
}`;

  it('applies unique exact match', () => {
    const { newContent, result, error } = applySearchReplace(
      sample,
      '  console.log("hello");',
      '  console.log("hi from zencode");'
    );

    expect(error).toBeUndefined();
    expect(result.applied).toBe(true);
    expect(newContent).toContain('console.log("hi from zencode")');
    expect(result.diff).toContain('+  console.log("hi from zencode")');
  });

  it('rejects when old_string not found', () => {
    const { error } = applySearchReplace(sample, 'nonexistent', 'foo');
    expect(error).toMatch(/not found/);
  });

  it('rejects when old_string appears multiple times', () => {
    const dup = `console.log(1);
console.log(1);`;
    const { error } = applySearchReplace(dup, 'console.log(1);', 'console.log(2);');
    expect(error).toMatch(/not unique/);
  });

  it('generates useful diff', () => {
    const { result } = applySearchReplace(sample, '  return 42;', '  return 99;');
    expect(result.diff).toContain('-  return 42;');
    expect(result.diff).toContain('+  return 99;');
  });

  it('preserves surrounding code on success', () => {
    const { newContent } = applySearchReplace(sample, '  return 42;', '  return 99;');
    expect(newContent).toContain('console.log("hello")');
    expect(newContent).toContain('return 99');
  });
});