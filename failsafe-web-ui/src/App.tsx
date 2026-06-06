import { Navigate, Route, Routes } from "react-router-dom"

import { AppLayout } from "@/components/AppLayout"
import { GuestRoute } from "@/components/GuestRoute"
import { ProtectedRoute } from "@/components/ProtectedRoute"
import { UnauthorizedHandler } from "@/components/UnauthorizedHandler"
import { DevicesPage } from "@/pages/DevicesPage"
import { LoginPage } from "@/pages/LoginPage"
import { NotFoundPage } from "@/pages/NotFoundPage"
import { RegisterPage } from "@/pages/RegisterPage"
import { isAuthenticated } from "@/lib/auth"

export function App() {
  return (
    <>
      <UnauthorizedHandler />
      <Routes>
        <Route element={<AppLayout />}>
          <Route
            path="/"
            element={
              <Navigate
                to={isAuthenticated() ? "/devices" : "/login"}
                replace
              />
            }
          />
          <Route
            path="/login"
            element={
              <GuestRoute>
                <LoginPage />
              </GuestRoute>
            }
          />
          <Route
            path="/register"
            element={
              <GuestRoute>
                <RegisterPage />
              </GuestRoute>
            }
          />
          <Route
            path="/devices"
            element={
              <ProtectedRoute>
                <DevicesPage />
              </ProtectedRoute>
            }
          />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Routes>
    </>
  )
}

export default App
