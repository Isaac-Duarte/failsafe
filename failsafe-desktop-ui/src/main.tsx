import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { BrowserRouter } from "react-router-dom"

import "../../failsafe-ui/src/index.css"
import App from "@/App"
import { ThemeProvider, Toaster, TooltipProvider } from "@failsafe/ui"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider defaultTheme="system">
      <TooltipProvider>
        <BrowserRouter>
          <App />
          <Toaster richColors closeButton position="top-right" />
        </BrowserRouter>
      </TooltipProvider>
    </ThemeProvider>
  </StrictMode>
)
