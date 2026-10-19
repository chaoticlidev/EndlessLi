/** Library type vocabulary, mirrored from `LIBRARY_TYPE_VALUES` in Rust. */
export const LIBRARY_TYPES = [
  "pdf",
  "paper",
  "article",
  "book",
  "video",
  "dataset",
  "podcast",
] as const;

export function typeLabel(itemType: string): string {
  switch (itemType) {
    case "pdf":
      return "PDF";
    case "paper":
      return "Paper";
    case "article":
      return "Article";
    case "book":
      return "Book";
    case "video":
      return "Video";
    case "dataset":
      return "Dataset";
    case "podcast":
      return "Podcast";
    default:
      return itemType;
  }
}
