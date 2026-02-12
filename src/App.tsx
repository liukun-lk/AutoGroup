import { Provider } from "jotai";
import { useAtom } from "jotai";
import { currentStepAtom, errorAtom, resetStateAtom } from "./stores";
import { UploadPage } from "@/components/features/UploadPage";
import { ConfigurePage } from "@/components/features/ConfigurePage";
import { ComputePage } from "@/components/features/ComputePage";
import { ResultsPage } from "@/components/features/ResultsPage";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { AlertCircle, X } from "lucide-react";

function AppContent() {
  const [currentStep] = useAtom(currentStepAtom);
  const [error, setError] = useAtom(errorAtom);
  const [, resetState] = useAtom(resetStateAtom);

  const renderStep = () => {
    switch (currentStep) {
      case "upload":
        return <UploadPage />;
      case "configure":
        return <ConfigurePage />;
      case "compute":
        return <ComputePage />;
      case "results":
        return <ResultsPage />;
      default:
        return <UploadPage />;
    }
  };

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="border-b">
        <div className="container max-w-7xl mx-auto px-4 py-4 flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold">AutoGroup</h1>
            <p className="text-sm text-muted-foreground">
              动物实验智能分组系统
            </p>
          </div>

          {/* Step Indicator */}
          <div className="flex items-center gap-2">
            <StepIndicator
              step={1}
              label="上传数据"
              active={currentStep === "upload"}
              completed={
                currentStep === "configure" ||
                currentStep === "compute" ||
                currentStep === "results"
              }
            />
            <div className="w-8 h-0.5 bg-border" />
            <StepIndicator
              step={2}
              label="配置参数"
              active={currentStep === "configure"}
              completed={
                currentStep === "compute" || currentStep === "results"
              }
            />
            <div className="w-8 h-0.5 bg-border" />
            <StepIndicator
              step={3}
              label="计算分组"
              active={currentStep === "compute"}
              completed={currentStep === "results"}
            />
            <div className="w-8 h-0.5 bg-border" />
            <StepIndicator
              step={4}
              label="查看结果"
              active={currentStep === "results"}
              completed={false}
            />
          </div>
        </div>
      </header>

      {/* Global Error Alert */}
      {error && (
        <div className="container max-w-7xl mx-auto px-4 pt-4">
          <Alert variant="destructive" className="relative">
            <AlertCircle className="h-4 w-4" />
            <AlertDescription className="pr-8">{error}</AlertDescription>
            <Button
              variant="ghost"
              size="sm"
              className="absolute right-2 top-2 h-6 w-6 p-0"
              onClick={() => setError(null)}
            >
              <X className="h-4 w-4" />
            </Button>
          </Alert>
        </div>
      )}

      {/* Main Content */}
      <main>{renderStep()}</main>
    </div>
  );
}

interface StepIndicatorProps {
  step: number;
  label: string;
  active: boolean;
  completed: boolean;
}

function StepIndicator({ step, label, active, completed }: StepIndicatorProps) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div
        className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium transition-colors ${completed
            ? "bg-primary text-primary-foreground"
            : active
              ? "bg-primary/10 text-primary border-2 border-primary"
              : "bg-muted text-muted-foreground"
          }`}
      >
        {completed ? "✓" : step}
      </div>
      <span
        className={`text-xs ${active ? "text-foreground font-medium" : "text-muted-foreground"
          }`}
      >
        {label}
      </span>
    </div>
  );
}

function App() {
  return (
    <Provider>
      <AppContent />
    </Provider>
  );
}

export default App;
