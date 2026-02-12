import { useState, useCallback } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { datasetAtom, currentStepAtom, setErrorAtom } from "@/stores";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Upload, FileSpreadsheet, CheckCircle2 } from "lucide-react";
import type { Dataset } from "@/types";

export function UploadPage() {
  const [dataset, setDataset] = useAtom(datasetAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [, setError] = useAtom(setErrorAtom);
  const [isLoading, setIsLoading] = useState(false);

  const handleFileSelect = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);

      // Open file dialog
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Excel Files',
          extensions: ['xlsx', 'xls']
        }]
      });

      if (!selected) {
        setIsLoading(false);
        return;
      }

      const filePath = typeof selected === 'string' ? selected : selected.path;

      // Parse Excel file via Tauri
      const result = await invoke<Dataset>("parse_excel", { filePath });

      setDataset(result);
      setCurrentStep("configure");
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoading(false);
    }
  }, [setDataset, setCurrentStep, setError]);

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
          {/* Upload Button */}
          <div className="flex flex-col items-center justify-center py-12 border-2 border-dashed rounded-lg bg-muted/50">
            <Upload className="h-12 w-12 text-muted-foreground mb-4" />
            <h3 className="text-lg font-semibold mb-2">选择 Excel 文件</h3>
            <p className="text-sm text-muted-foreground mb-6 text-center max-w-sm">
              文件应包含"原始数据" sheet，格式参考测试数据
            </p>
            <Button
              onClick={handleFileSelect}
              disabled={isLoading}
              size="lg"
            >
              <FileSpreadsheet className="mr-2 h-4 w-4" />
              {isLoading ? "解析中..." : "选择文件"}
            </Button>
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
