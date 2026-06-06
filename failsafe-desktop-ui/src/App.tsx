import { Navigate, Route, Routes } from "react-router-dom"

import { ScreenSharePage } from "@/pages/ScreenSharePage"

export function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/screen-share" replace />} />
      <Route path="/screen-share/:deviceId" element={<ScreenSharePage />} />
      <Route path="*" element={<Navigate to="/screen-share" replace />} />
    </Routes>
  )
}

export default App
