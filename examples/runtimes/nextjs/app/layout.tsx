import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "better-pdf — Next.js example",
  description: "Generate PDFs in the browser with @ignaciano3/better-pdf",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
