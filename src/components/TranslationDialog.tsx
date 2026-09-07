import { invoke } from "@tauri-apps/api/core";
import { Cloud, Cpu, Languages, LoaderCircle, Save, Trash2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { SummaryPreferences, TranscriptSegment, TranslationTarget } from "../types";

export type RetryPreferences = Pick<SummaryPreferences,
  "ollamaBaseUrl" | "ollamaTranslationModel" | "ollamaCloudTranslationModel">;

export function TranslationDialog({ segment, preferences, onSave, onTranslate, onClose }: {
  segment: TranscriptSegment;
  preferences: SummaryPreferences;
  onSave: (patch: RetryPreferences) => void;
  onTranslate: (target: TranslationTarget) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [target, setTarget] = useState<TranslationTarget>("configured");
  const [baseUrl, setBaseUrl] = useState(preferences.ollamaBaseUrl);
  const [localModel, setLocalModel] = useState(preferences.ollamaTranslationModel);
  const [cloudModel, setCloudModel] = useState(preferences.ollamaCloudTranslationModel);
  const [apiKey, setApiKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [checkingKey, setCheckingKey] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const previouslyFocused = document.activeElement;
    dialog.current?.showModal();
    let active = true;
    void invoke<boolean>("has_provider_api_key", { provider: "ollama-cloud" })
      .then((value) => { if (active) setHasKey(value); })
      .catch(() => { if (active) setHasKey(false); })
      .finally(() => { if (active) setCheckingKey(false); });
    return () => {
      active = false;
      if (previouslyFocused instanceof HTMLElement && previouslyFocused.isConnected) previouslyFocused.focus();
    };
  }, []);

  const local = target === "ollama-local";
  const cloud = target === "ollama-cloud";
  const dirty = local
    ? baseUrl.trim() !== preferences.ollamaBaseUrl || localModel.trim() !== preferences.ollamaTranslationModel
    : cloud && (cloudModel.trim() !== preferences.ollamaCloudTranslationModel || Boolean(apiKey.trim()));
  const valid = local ? Boolean(baseUrl.trim() && localModel.trim())
    : cloud ? Boolean(cloudModel.trim() && (hasKey || apiKey.trim())) : true;

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      if (local) {
        const parsed = new URL(baseUrl.trim());
        if (!["http:", "https:"].includes(parsed.protocol) || parsed.username || parsed.password || parsed.search || parsed.hash) {
          throw new Error("本地服务地址需要有效的 HTTP/HTTPS 地址，且不能包含凭据或查询参数");
        }
      }
      if (cloud && apiKey.trim()) {
        await invoke("save_provider_api_key", { provider: "ollama-cloud", apiKey: apiKey.trim() });
        setHasKey(true);
        setApiKey("");
      }
      onSave({
        ollamaBaseUrl: local ? baseUrl.trim() : preferences.ollamaBaseUrl,
        ollamaTranslationModel: local ? localModel.trim() : preferences.ollamaTranslationModel,
        ollamaCloudTranslationModel: cloud ? cloudModel.trim() : preferences.ollamaCloudTranslationModel,
      });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  const deleteKey = async () => {
    setSaving(true);
    setError(null);
    try {
      await invoke("delete_provider_api_key", { provider: "ollama-cloud" });
      setHasKey(false);
      setApiKey("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <dialog ref={dialog} className="translation-dialog" aria-labelledby="translation-dialog-title"
      onCancel={(event) => { event.preventDefault(); if (!saving) onClose(); }}>
      <header className="dialog-header">
        <h2 id="translation-dialog-title">语段补翻</h2>
        <button className="icon-button" type="button" title="关闭补翻" aria-label="关闭补翻" disabled={saving} onClick={onClose}><X size={18} /></button>
      </header>
      <p className="retry-source">{segment.english}</p>
      <div className="retry-fields">
        <label htmlFor="retry-target">翻译来源</label>
        <select id="retry-target" value={target} disabled={saving} autoFocus
          onChange={(event) => { setTarget(event.target.value as TranslationTarget); setError(null); }}>
          <option value="configured">当前云端配置</option>
          <option value="ollama-local">Ollama 本地</option>
          <option value="ollama-cloud">Ollama 云端</option>
        </select>
        {target === "configured" && <div className="retry-provider-detail"><Cloud size={16} /><span>{preferences.textProvider === "alibaba" ? "阿里云百炼" : "OpenAI"} · {preferences.translationModel}</span></div>}
        {local && <>
          <label htmlFor="retry-base-url">本地服务地址</label>
          <input id="retry-base-url" type="url" maxLength={512} spellCheck={false} autoComplete="off" value={baseUrl} disabled={saving} onChange={(event) => setBaseUrl(event.target.value)} />
          <label htmlFor="retry-local-model">本地模型</label>
          <input id="retry-local-model" list="retry-local-models" maxLength={100} spellCheck={false} value={localModel} disabled={saving} onChange={(event) => setLocalModel(event.target.value)} />
          <datalist id="retry-local-models"><option value="qwen2.5:3b" /><option value="qwen2.5:7b" /><option value="qwen3:0.6b" /></datalist>
          <div className="retry-provider-detail"><Cpu size={16} /><span>无需 API Key</span></div>
        </>}
        {cloud && <>
          <div className="retry-provider-detail"><Cloud size={16} /><span>https://ollama.com · 使用云端额度</span></div>
          <label htmlFor="retry-cloud-model">云端模型 ID</label>
          <input id="retry-cloud-model" maxLength={100} spellCheck={false} value={cloudModel} disabled={saving} onChange={(event) => setCloudModel(event.target.value)} />
          <div className="label-row"><label htmlFor="retry-cloud-key">Ollama Cloud API Key</label><span className={`key-state ${hasKey ? "saved" : "missing"}`}>{checkingKey ? "检查中" : hasKey ? "已保存至系统凭据库" : "未保存"}</span></div>
          <div className="key-input-row">
            <input id="retry-cloud-key" type="password" autoComplete="off" value={apiKey} disabled={saving || checkingKey} placeholder={hasKey ? "输入新密钥可替换" : "API Key"} onChange={(event) => setApiKey(event.target.value)} />
            {hasKey && <button className="icon-button danger-quiet" type="button" disabled={saving} title="删除 Ollama 云端 Key" aria-label="删除 Ollama 云端 Key" onClick={() => void deleteKey()}><Trash2 size={17} /></button>}
          </div>
        </>}
        {error && <p className="translation-error" role="alert">{error}</p>}
      </div>
      <footer className="dialog-actions">
        {target !== "configured" && <button className="secondary-button" type="button" disabled={saving || !dirty || !valid || (cloud && checkingKey)} onClick={() => void save()}>
          {saving ? <LoaderCircle size={15} className="spin" /> : <Save size={15} />}保存配置
        </button>}
        <button className="primary-button" type="button" disabled={saving || dirty || !valid || (cloud && (checkingKey || !hasKey)) || segment.state === "translating"}
          onClick={() => onTranslate(target)}><Languages size={16} />翻译此语段</button>
      </footer>
    </dialog>
  );
}
