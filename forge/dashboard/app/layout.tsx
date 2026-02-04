import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Zelana Prover Dashboard",
  description: "Distributed Zero-Knowledge Proof System",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className="antialiased">
        {children}
      </body>
    </html>
  );
}
