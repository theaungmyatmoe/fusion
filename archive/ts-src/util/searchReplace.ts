/**
 * Zencode search_replace safety util.
 * Ported/adapted from the Go version.
 * The critical rule: oldString MUST appear exactly once.
 * This prevents ambiguous or accidental multi-file-damage edits.
 */

export interface SearchReplaceResult {
  oldText: string;
  newText: string;
  diff: string;
  applied: boolean;
}

export function applySearchReplace(
  content: string,
  oldString: string,
  newString: string
): { newContent: string; result: SearchReplaceResult; error?: string } {
  if (!oldString) {
    return {
      newContent: content,
      result: { oldText: oldString, newText: newString, diff: "", applied: false },
      error: "old_string must not be empty",
    };
  }

  const count = countOccurrences(content, oldString);

  if (count === 0) {
    return {
      newContent: content,
      result: { oldText: oldString, newText: newString, diff: "", applied: false },
      error: "old_string not found in file",
    };
  }

  if (count > 1) {
    return {
      newContent: content,
      result: { oldText: oldString, newText: newString, diff: "", applied: false },
      error: `old_string is not unique (appears ${count} times) — make the string longer and more specific`,
    };
  }

  const newContent = content.replace(oldString, newString);

  return {
    newContent,
    result: {
      oldText: oldString,
      newText: newString,
      diff: makeSimpleDiff(oldString, newString),
      applied: true,
    },
  };
}

function countOccurrences(haystack: string, needle: string): number {
  let count = 0;
  let pos = 0;
  while (true) {
    const idx = haystack.indexOf(needle, pos);
    if (idx === -1) break;
    count++;
    pos = idx + 1; // allow overlapping in theory, but for code usually not
  }
  return count;
}

function makeSimpleDiff(oldStr: string, newStr: string): string {
  const oldLines = oldStr.split("\n");
  const newLines = newStr.split("\n");
  const max = Math.max(oldLines.length, newLines.length);

  let out = "```diff\n";
  for (let i = 0; i < max; i++) {
    const o = oldLines[i] ?? "";
    const n = newLines[i] ?? "";

    if (o === n) {
      out += " " + o + "\n";
    } else {
      if (o) out += "-" + o + "\n";
      if (n || !o) out += "+" + n + "\n";
    }
  }
  out += "```";
  return out;
}
