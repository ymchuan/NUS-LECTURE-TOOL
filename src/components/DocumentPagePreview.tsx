import { FileText, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { PDFDocumentLoadingTask, PDFDocumentProxy, RenderTask } from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import type { DocumentSource } from "../types";

function decodeBase64(value: string) {
  const binary = window.atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function arrayBufferFor(bytes: Uint8Array) {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function PreviewStatus({ label }: { label: string }) {
  return (
    <div className="document-preview-status" role="status">
      <LoaderCircle className="spin" size={20} />
      <span>{label}</span>
    </div>
  );
}

function PreviewError({ message, fallbackText }: { message: string; fallbackText: string }) {
  return (
    <div className="document-preview-error">
      <FileText size={24} />
      <strong>原页预览暂时无法显示</strong>
      <span>{message}</span>
      {!!fallbackText.trim() && (
        <details>
          <summary>查看备用文字</summary>
          <div>{fallbackText}</div>
        </details>
      )}
    </div>
  );
}

function PdfPagePreview({ source, pageNumber, fallbackText }: PreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  const [loadError, setLoadError] = useState("");
  const [renderError, setRenderError] = useState("");
  const [rendering, setRendering] = useState(true);

  useEffect(() => {
    let cancelled = false;
    let loadingTask: PDFDocumentLoadingTask | null = null;
    const load = async () => {
      setPdf(null);
      setLoadError("");
      setRendering(true);
      try {
        const pdfjs = await import("pdfjs-dist");
        pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
        loadingTask = pdfjs.getDocument({
          data: decodeBase64(source.dataBase64),
          isEvalSupported: false,
        });
        const loadedDocument = await loadingTask.promise;
        if (!cancelled) setPdf(loadedDocument);
      } catch (reason) {
        if (!cancelled) setLoadError(String(reason));
      }
    };
    void load();
    return () => {
      cancelled = true;
      void loadingTask?.destroy();
    };
  }, [source.dataBase64]);

  useEffect(() => {
    if (!pdf) return;
    let cancelled = false;
    let renderTask: RenderTask | null = null;
    const render = async () => {
      setRendering(true);
      setRenderError("");
      try {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const page = await pdf.getPage(Math.min(Math.max(1, pageNumber), pdf.numPages));
        const unscaled = page.getViewport({ scale: 1 });
        const availableWidth = Math.min(canvas.parentElement?.clientWidth ?? 960, 1120);
        const viewport = page.getViewport({ scale: Math.max(0.7, availableWidth / unscaled.width) });
        const outputScale = Math.min(window.devicePixelRatio || 1, 2);
        const context = canvas.getContext("2d", { alpha: false });
        if (!context) throw new Error("无法创建 PDF 画布");
        canvas.width = Math.floor(viewport.width * outputScale);
        canvas.height = Math.floor(viewport.height * outputScale);
        canvas.style.width = `${Math.floor(viewport.width)}px`;
        canvas.style.height = `${Math.floor(viewport.height)}px`;
        renderTask = page.render({
          canvasContext: context,
          viewport,
          transform: outputScale === 1 ? undefined : [outputScale, 0, 0, outputScale, 0, 0],
        });
        await renderTask.promise;
        if (!cancelled) setRendering(false);
      } catch (reason) {
        if (!cancelled && String(reason).toLowerCase().includes("cancel") === false) {
          setRenderError(String(reason));
          setRendering(false);
        }
      }
    };
    void render();
    return () => {
      cancelled = true;
      renderTask?.cancel();
    };
  }, [pageNumber, pdf]);

  if (loadError || renderError) {
    return <PreviewError message={loadError || renderError} fallbackText={fallbackText} />;
  }
  return (
    <div className="pdf-page-preview" aria-label="PDF 原页预览">
      {(!pdf || rendering) && <PreviewStatus label={!pdf ? "正在读取 PDF" : "正在渲染这一页"} />}
      <canvas ref={canvasRef} className={rendering ? "rendering" : ""} aria-label={`PDF 第 ${pageNumber} 页`} />
    </div>
  );
}

type PptxPreviewer = {
  slideCount: number;
  load: (file: ArrayBuffer) => Promise<unknown>;
  renderSingleSlide: (slideIndex: number) => void;
  destroy: () => void;
};

function PptxPagePreview({ source, pageNumber, fallbackText }: PreviewProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const previewerRef = useRef<PptxPreviewer | null>(null);
  const latestPageRef = useRef(pageNumber);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  latestPageRef.current = pageNumber;

  useEffect(() => {
    let cancelled = false;
    const host = hostRef.current;
    if (!host) return;
    const render = async () => {
      setLoading(true);
      setError("");
      host.replaceChildren();
      try {
        const { init } = await import("pptx-preview");
        const width = Math.max(320, Math.floor(host.clientWidth || 920));
        const previewer = init(host, {
          width,
          height: Math.round(width * 0.75),
          mode: "slide",
        }) as PptxPreviewer;
        previewerRef.current = previewer;
        await previewer.load(arrayBufferFor(decodeBase64(source.dataBase64)));
        if (cancelled) {
          previewer.destroy();
          return;
        }
        const page = Math.min(Math.max(1, latestPageRef.current), previewer.slideCount || 1);
        previewer.renderSingleSlide(page - 1);
        setLoading(false);
      } catch (reason) {
        if (!cancelled) {
          previewerRef.current?.destroy();
          previewerRef.current = null;
          host.replaceChildren();
          setError(String(reason));
          setLoading(false);
        }
      }
    };
    void render();
    return () => {
      cancelled = true;
      previewerRef.current?.destroy();
      previewerRef.current = null;
      host.replaceChildren();
    };
  }, [source.dataBase64]);

  useEffect(() => {
    const previewer = previewerRef.current;
    if (!previewer || loading) return;
    const page = Math.min(Math.max(1, pageNumber), previewer.slideCount || 1);
    previewer.renderSingleSlide(page - 1);
  }, [loading, pageNumber]);

  if (error) return <PreviewError message={error} fallbackText={fallbackText} />;
  return (
    <div className="pptx-page-preview" aria-label="PowerPoint 原页预览">
      {loading && <PreviewStatus label="正在还原 PowerPoint 页面" />}
      <div ref={hostRef} className={loading ? "rendering" : ""} />
    </div>
  );
}

type PreviewProps = {
  source: DocumentSource;
  pageNumber: number;
  fallbackText: string;
};

export function DocumentPagePreview(props: PreviewProps) {
  const isPdf = props.source.mimeType === "application/pdf"
    || props.source.name.toLowerCase().endsWith(".pdf");
  return isPdf ? <PdfPagePreview {...props} /> : <PptxPagePreview {...props} />;
}
