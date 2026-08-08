import { useState, useCallback, useEffect } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { datasetAtom, currentStepAtom, clearErrorAtom, resetStateAtom } from "@/stores";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Upload, FileSpreadsheet, CheckCircle2, AlertCircle, History, X } from "lucide-react";
import type { Dataset } from "@/types";
import {
  loadRecentImports,
  recordRecentImport,
  removeRecentImport,
  formatImportTime,
  type RecentImport,
} from "@/lib/recent-files";

export function UploadPage() {
  const [dataset, setDataset] = useAtom(datasetAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [, clearError] = useAtom(clearErrorAtom);
  const [, resetState] = useAtom(resetStateAtom);
  const [isLoading, setIsLoading] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [dragFileValid, setDragFileValid] = useState<boolean | null>(null);
  const [recentImports, setRecentImports] = useState<RecentImport[]>(() => loadRecentImports());

  const isExcelFile = (path: string) => /\.(xlsx|xlsm|xls)$/i.test(path);

  const describeUnsupportedFile = (path: string, source: "拖拽" | "粘贴") => {
    const fileName = path.split(/[\\/]/).pop() || path;
    const ext = fileName.includes(".") ? fileName.split(".").pop()!.toLowerCase() : "";
    const formatDesc = ext ? `.${ext} 格式` : "无扩展名";
    return `${source}的文件「${fileName}」是${formatDesc}，暂不支持。\n请上传 .xlsx 格式的 Excel 文件；若是 .xls 或 .csv，请先用 Excel 另存为 .xlsx。`;
  };

  const handleFileParse = useCallback(async (filePath: string) => {
    try {
      resetState();
      setIsLoading(true);
      clearError();
      setLocalError(null);

      // Parse Excel file via Tauri
      const result = await invoke<Dataset>("parse_excel", { filePath });

      setRecentImports(recordRecentImport(filePath));
      setDataset(result);
      setCurrentStep("configure");
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      setLocalError(errorMessage);
    } finally {
      setIsLoading(false);
    }
  }, [resetState, setDataset, setCurrentStep, clearError]);

  const handleFileSelect = useCallback(async () => {
    try {
      setIsLoading(true);
      clearError();
      setLocalError(null);

      // Open file dialog
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Excel 文件',
          extensions: ['xlsx', 'xlsm', 'xls']
        }]
      });

      if (!selected) {
        setIsLoading(false);
        return;
      }

      const filePath = typeof selected === 'string' ? selected : (selected as { path: string }).path;
      await handleFileParse(filePath);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      setLocalError(`打开文件选择框失败：${errorMessage}`);
      setIsLoading(false);
    }
  }, [clearError, handleFileParse]);

  // Setup drag-drop listener
  useEffect(() => {
    let unlistenFn: (() => void) | null = null;

    const setupDragDrop = async () => {
      try {
        const appWindow = getCurrentWindow();
        unlistenFn = await appWindow.onDragDropEvent((event) => {
          const dragEvent = event.payload as DragDropEvent;

          if (dragEvent.type === 'over' || dragEvent.type === 'enter') {
            setIsDragOver(true);
            if (dragEvent.type === 'enter') {
              const path = dragEvent.paths?.[0];
              if (path) {
                setDragFileValid(isExcelFile(path));
              }
            }
            return;
          }

          if (dragEvent.type === 'leave') {
            setIsDragOver(false);
            setDragFileValid(null);
            return;
          }

          if (dragEvent.type === 'drop') {
            setIsDragOver(false);
            setDragFileValid(null);

            const paths = dragEvent.paths;
            if (!paths || paths.length === 0 || isLoading) {
              return;
            }

            const filePath = paths[0];
            if (isExcelFile(filePath)) {
              handleFileParse(filePath);
            } else {
              setLocalError(describeUnsupportedFile(filePath, "拖拽"));
            }
          }
        });
      } catch (err) {
        console.error("Failed to setup drag-drop listener:", err);
      }
    };

    setupDragDrop();

    return () => {
      unlistenFn?.();
    };
  }, [isLoading, handleFileParse]);

  // Setup clipboard paste listener
  useEffect(() => {
    const handlePaste = async (e: ClipboardEvent) => {
      e.preventDefault();

      if (isLoading) {
        return;
      }

      try {
        setIsLoading(true);
        clearError();
        setLocalError(null);

        // Call Rust command to get file paths from clipboard
        const paths = await invoke<string[]>("parse_clipboard_files");

        if (paths.length === 0) {
          setLocalError("剪贴板中没有文件。\n请先在访达 / 资源管理器中复制 Excel 文件，再回到本页面粘贴。");
          setIsLoading(false);
          return;
        }

        const filePath = paths[0];
        if (isExcelFile(filePath)) {
          await handleFileParse(filePath);
        } else {
          setLocalError(describeUnsupportedFile(filePath, "粘贴"));
          setIsLoading(false);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        setLocalError(`从剪贴板读取文件失败：${errorMessage}`);
        setIsLoading(false);
      }
    };

    document.addEventListener("paste", handlePaste);

    return () => {
      document.removeEventListener("paste", handlePaste);
    };
  }, [isLoading, clearError, handleFileParse]);

  return (
    <div className="container max-w-4xl mx-auto py-8">
      <Card>
        <CardHeader>
          <CardTitle className="text-2xl">上传数据文件</CardTitle>
          <CardDescription>
            选择包含动物实验数据的 Excel 文件 (.xlsx)
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* Local Error Alert */}
          {localError && (
            <Alert variant="destructive">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                <div className="font-semibold mb-1">文件导入失败</div>
                <div className="whitespace-pre-line leading-relaxed">{localError}</div>
                <div className="mt-2 text-xs opacity-80">
                  可对照下方「文件格式要求」逐项检查。
                </div>
              </AlertDescription>
            </Alert>
          )}

          {/* Upload Button */}
          <div
            className={`
              border-2 border-dashed rounded-lg py-12 transition-all
              ${isDragOver && dragFileValid === true
                ? 'border-green-500 bg-green-50 dark:bg-green-950/30'
                : isDragOver && dragFileValid === false
                  ? 'border-red-500 bg-red-50 dark:bg-red-950/30'
                  : 'border-border bg-muted/50'
              }
            `}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => e.preventDefault()}
          >
            <div className="flex flex-col items-center justify-center">
              <Upload className="h-12 w-12 text-muted-foreground mb-4" />

              {isDragOver && dragFileValid === true && (
                <h3 className="text-lg font-semibold mb-2 text-green-700 dark:text-green-400">
                  松开即可上传
                </h3>
              )}

              {isDragOver && dragFileValid === false && (
                <h3 className="text-lg font-semibold mb-2 text-red-700 dark:text-red-400">
                  只支持 .xlsx 格式的 Excel 文件
                </h3>
              )}

              {!isDragOver && (
                <>
                  <h3 className="text-lg font-semibold mb-2">选择 Excel 文件</h3>
                  <p className="text-sm text-muted-foreground mb-4 text-center max-w-sm">
                    第一个 sheet 需名为「原始数据」，排版见下方格式要求
                  </p>
                  <p className="text-xs text-muted-foreground mb-6 text-center max-w-md">
                    支持拖拽文件到此区域，或按 Cmd+V / Ctrl+V 粘贴已复制的文件
                  </p>
                </>
              )}

              {!isDragOver && (
                <Button
                  onClick={handleFileSelect}
                  disabled={isLoading}
                  size="lg"
                >
                  <FileSpreadsheet className="mr-2 h-4 w-4" />
                  {isLoading ? "解析中…" : "选择文件"}
                </Button>
              )}
            </div>
          </div>

          {/* Recent Imports */}
          {recentImports.length > 0 && (
            <div className="border rounded-lg p-4">
              <div className="flex items-center gap-2 mb-3">
                <History className="h-4 w-4 text-muted-foreground" />
                <h4 className="text-sm font-medium">最近导入</h4>
                <span className="text-xs text-muted-foreground">点击文件名可直接重新导入</span>
              </div>
              <ul className="space-y-1">
                {recentImports.map((item) => (
                  <li key={item.path} className="flex items-center gap-2 group">
                    <button
                      type="button"
                      onClick={() => handleFileParse(item.path)}
                      disabled={isLoading}
                      title={item.path}
                      className="flex-1 flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      <FileSpreadsheet className="h-4 w-4 shrink-0 text-green-600" />
                      <span className="truncate font-medium">{item.name}</span>
                      <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">
                        {formatImportTime(item.importedAt)}
                      </span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setRecentImports(removeRecentImport(item.path))}
                      disabled={isLoading}
                      title="从最近导入中移除"
                      className="shrink-0 rounded-md p-1.5 text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-muted hover:text-foreground disabled:opacity-0 transition-opacity"
                    >
                      <X className="h-3.5 w-3.5" />
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Dataset Info */}
          {dataset && (
            <Alert className="border-green-200 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                <div className="font-semibold mb-2">文件解析成功</div>
                <div className="grid grid-cols-2 gap-2 text-sm">
                  <div>总动物数: <span className="font-medium">{dataset.metadata.total_animals}</span></div>
                  <div>指标数量: <span className="font-medium">{dataset.metadata.indicator_count}</span></div>
                  <div>雄性: <span className="font-medium">{dataset.metadata.male_count}</span></div>
                  <div>雌性: <span className="font-medium">{dataset.metadata.female_count}</span></div>
                </div>
              </AlertDescription>
            </Alert>
          )}

          {/* Requirements */}
          <div className="bg-blue-50 border border-blue-200 rounded-lg p-4">
            <h4 className="font-medium text-blue-900 mb-1">文件格式要求：</h4>
            <p className="text-sm text-blue-800 mb-3">
              文件须为 .xlsx 格式（旧版 .xls 请先在 Excel 中另存为 .xlsx），排版如下：
            </p>

            {/* Mock Excel preview */}
            <div className="overflow-x-auto">
              <table className="border-collapse text-xs whitespace-nowrap">
                <thead>
                  <tr className="text-slate-500 font-normal">
                    <th className="border border-slate-300 bg-slate-100 px-2 py-1 font-normal w-8" />
                    {["A", "B", "C", "D", "E"].map((col) => (
                      <th key={col} className="border border-slate-300 bg-slate-100 px-2 py-1 font-normal">
                        {col}
                      </th>
                    ))}
                    <th className="border border-slate-300 bg-slate-100 px-2 py-1 font-normal">⋯</th>
                    <th />
                  </tr>
                </thead>
                <tbody className="text-slate-700">
                  <tr>
                    <td className="border border-slate-300 bg-slate-100 px-2 py-1 text-center text-slate-500">1</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 italic text-slate-400">（留空）</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 italic text-slate-400">（留空）</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1">kg</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1">℃</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1">ALT</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 text-center text-slate-400">⋯</td>
                    <td className="pl-3 text-blue-700">← 第 1 行：英文指标名或单位</td>
                  </tr>
                  <tr>
                    <td className="border border-slate-300 bg-slate-100 px-2 py-1 text-center text-slate-500">2</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 font-medium">动物编号</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 font-medium">性别</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 font-medium">体重</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 font-medium">肛温</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 font-medium">U/L</td>
                    <td className="border border-slate-300 bg-amber-50 px-2 py-1 text-center text-slate-400">⋯</td>
                    <td className="pl-3 text-blue-700">← 第 2 行：中文列名或单位</td>
                  </tr>
                  <tr>
                    <td className="border border-slate-300 bg-slate-100 px-2 py-1 text-center text-slate-500">3</td>
                    <td className="border border-slate-300 bg-green-50 px-2 py-1">XHP2601001</td>
                    <td className="border border-slate-300 bg-purple-50 px-2 py-1 text-center">F</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">31.85</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">38.5</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">58.8</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-center text-slate-400">⋯</td>
                    <td className="pl-3 text-blue-700">← 第 3 行起：一行一只动物</td>
                  </tr>
                  <tr>
                    <td className="border border-slate-300 bg-slate-100 px-2 py-1 text-center text-slate-500">4</td>
                    <td className="border border-slate-300 bg-green-50 px-2 py-1">XHP2601002</td>
                    <td className="border border-slate-300 bg-purple-50 px-2 py-1 text-center">M</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">30.45</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">38.5</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-right">42.2</td>
                    <td className="border border-slate-300 bg-white px-2 py-1 text-center text-slate-400">⋯</td>
                    <td />
                  </tr>
                  <tr className="text-slate-300">
                    <td className="border border-slate-300 bg-slate-100 px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-green-50 px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-purple-50 px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-white px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-white px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-white px-2 py-0.5 text-center">⋮</td>
                    <td className="border border-slate-300 bg-white px-2 py-0.5 text-center">⋮</td>
                    <td />
                  </tr>
                </tbody>
              </table>

              {/* Sheet tab, like the bottom of an Excel window */}
              <div className="flex items-center text-xs">
                <div className="flex items-center border border-t-0 border-slate-300 bg-slate-100 rounded-b-sm">
                  <span className="bg-white border-r border-slate-300 px-3 py-0.5 font-medium text-slate-700 rounded-bl-sm">
                    原始数据
                  </span>
                  <span className="px-2 text-slate-400">+</span>
                </div>
                <span className="pl-3 text-blue-700">← 第一个 sheet 名为「原始数据」</span>
              </div>
            </div>

            {/* Column legend */}
            <div className="flex flex-wrap gap-x-4 gap-y-1.5 mt-3 text-xs text-blue-800">
              <span className="flex items-center gap-1.5">
                <span className="inline-block h-3 w-3 rounded-sm border border-amber-300 bg-amber-50" />
                第 1–2 行：表头
              </span>
              <span className="flex items-center gap-1.5">
                <span className="inline-block h-3 w-3 rounded-sm border border-green-300 bg-green-50" />
                A 列：动物编号（不可重复）
              </span>
              <span className="flex items-center gap-1.5">
                <span className="inline-block h-3 w-3 rounded-sm border border-purple-300 bg-purple-50" />
                B 列：性别（F/M 或 雌性/雄性）
              </span>
              <span className="flex items-center gap-1.5">
                <span className="inline-block h-3 w-3 rounded-sm border border-slate-300 bg-white" />
                C 列起：指标数值
              </span>
            </div>
          </div>

          {/* Next Button */}
          {dataset && (
            <div className="flex justify-end">
              <Button
                onClick={() => setCurrentStep("configure")}
                size="lg"
              >
                下一步：配置分组参数
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
