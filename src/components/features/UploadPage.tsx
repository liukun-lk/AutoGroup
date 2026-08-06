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
import { Upload, FileSpreadsheet, CheckCircle2, AlertCircle } from "lucide-react";
import type { Dataset } from "@/types";

export function UploadPage() {
  const [dataset, setDataset] = useAtom(datasetAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [, clearError] = useAtom(clearErrorAtom);
  const [, resetState] = useAtom(resetStateAtom);
  const [isLoading, setIsLoading] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [dragFileValid, setDragFileValid] = useState<boolean | null>(null);

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
                  可对照下方「文件格式要求」检查文件，或参考测试数据的排版方式。
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
                  ✅ 松开以上传 Excel 文件
                </h3>
              )}

              {isDragOver && dragFileValid === false && (
                <h3 className="text-lg font-semibold mb-2 text-red-700 dark:text-red-400">
                  ❌ 仅支持 Excel 文件（.xlsx）
                </h3>
              )}

              {!isDragOver && (
                <>
                  <h3 className="text-lg font-semibold mb-2">选择 Excel 文件</h3>
                  <p className="text-sm text-muted-foreground mb-4 text-center max-w-sm">
                    文件应包含"原始数据" sheet，格式参考测试数据
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
                  {isLoading ? "解析中..." : "选择文件"}
                </Button>
              )}
            </div>
          </div>

          {/* Dataset Info */}
          {dataset && (
            <Alert className="border-green-200 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                <div className="font-semibold mb-2">文件解析成功！</div>
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
            <h4 className="font-medium text-blue-900 mb-2">文件格式要求：</h4>
            <ul className="text-sm text-blue-800 space-y-1">
              <li>• 文件格式为 .xlsx（旧版 .xls 请先在 Excel 中另存为 .xlsx）</li>
              <li>• 第一个 sheet 为"原始数据"</li>
              <li>• Row 1: 英文指标名或单位（kg, ℃, ALT...）</li>
              <li>• Row 2: 中文列名或单位（体重, 肛温, U/L...）</li>
              <li>• Row 3+: 数据行</li>
              <li>• Column 1: 动物编号</li>
              <li>• Column 2: 性别（F/M 或 雌性/雄性）</li>
              <li>• Column 3+: 指标数值</li>
            </ul>
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
