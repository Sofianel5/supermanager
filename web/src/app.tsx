import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { LandingPage } from "./routes/landing-page";
import { RoomPage } from "./routes/room-page";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<LandingPage />} />
        <Route path="/r/:roomId" element={<RoomPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
