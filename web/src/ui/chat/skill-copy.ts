/** User-facing skill labels come from the skill frontmatter, never from a name map. */
export function skillDisplay(name: string, title?: string, description?: string): { title: string; description: string } {
  return {
    title: title || name || '作业规范',
    description: description || '',
  };
}
