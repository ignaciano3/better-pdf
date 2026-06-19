/** A bookmark / outline entry in a PDF document. */
export interface OutlineItem {
  /** Bookmark title shown in the PDF viewer. */
  title: string;
  /** Zero-based page index the bookmark navigates to. */
  page: number;
  /** Nested child bookmarks. */
  children?: OutlineItem[];
}
