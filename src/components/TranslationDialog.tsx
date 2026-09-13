import { Languages, LoaderCircle, X } from "lucide-react";
import type { TranscriptSegment } from "../types";

export function TranslationDialog({ segment, busy, error, onTranslate, onClose }: { segment: TranscriptSegment | null; busy: boolean; error: string | null; onTranslate: () => void; onClose: () => void }) {
  if (!segment) return null;
  return <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}><section className="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="translation-dialog-title" onMouseDown={(event) => event.stopPropagation()}><header className="dialog-header"><div><p className="eyebrow">历史课堂</p><h2 id="translation-dialog-title">重新翻译语段</h2></div><button className="icon-button" type="button" title="关闭" onClick={onClose} disabled={busy}><X size={18} /></button></header><p className="retry-source">{segment.english}</p>{error && <p className="translation-error" role="alert">{error}</p>}<div className="dialog-actions"><button className="secondary-button" type="button" onClick={onClose} disabled={busy}>取消</button><button className="primary-button" type="button" onClick={onTranslate} disabled={busy}>{busy ? <LoaderCircle className="spin" size={15} /> : <Languages size={15} />}{busy ? "翻译中" : "使用当前翻译配置"}</button></div></section></div>;
}
