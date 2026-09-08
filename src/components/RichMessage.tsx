import { openUrl } from "@tauri-apps/plugin-opener";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import { normalizeModelMarkdown } from "../lib/markdown";

const isTauri = () => "__TAURI_INTERNALS__" in window;

async function openExternalLink(url: string) {
  if (!/^https?:\/\//i.test(url)) return;
  if (isTauri()) await openUrl(url);
  else window.open(url, "_blank", "noopener,noreferrer");
}

export function RichMessage({ content }: { content: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath, remarkBreaks]}
      rehypePlugins={[[rehypeKatex, { strict: false, throwOnError: false }]]}
      skipHtml
      components={{
        a: ({ href, children }) => (
          <a
            href={href}
            target="_blank"
            rel="noreferrer"
            onClick={(event) => {
              event.preventDefault();
              if (href) void openExternalLink(href);
            }}
          >
            {children}
          </a>
        ),
      }}
    >
      {normalizeModelMarkdown(content)}
    </ReactMarkdown>
  );
}
