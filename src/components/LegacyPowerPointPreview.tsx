import { FileText } from "lucide-react";
import { RichMessage } from "./RichMessage";
import type { DocumentSource } from "../types";

type Props = {
  source: DocumentSource;
  pageNumber: number;
  fallbackText: string;
};

export default function LegacyPowerPointPreview({ source, pageNumber, fallbackText }: Props) {
  return (
    <div className="legacy-ppt-page-preview" aria-label={`PowerPoint 第 ${pageNumber} 页文字预览`}>
      <div className="legacy-ppt-preview-heading">
        <span><FileText size={14} />PPT 97-2003</span>
        <span>{pageNumber} / {source.pageCount}</span>
      </div>
      <div className="legacy-ppt-preview-page">
        {fallbackText.trim()
          ? <RichMessage content={fallbackText} />
          : <p>这一页没有可提取的文字内容。</p>}
      </div>
      <p className="legacy-ppt-preview-note">旧版 PPT 以逐页文字方式预览；热词提取、页面匹配和 AI 问答仍会使用这些内容。</p>
    </div>
  );
}
