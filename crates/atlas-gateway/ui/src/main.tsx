// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { Login } from "./views/Login";
import { useUi } from "./store/ui";
import { Toaster } from "./ui/Toaster";
import { ConfirmHost } from "./ui/confirm";
import "./index.css";

function Root() {
  const entered = useUi((s) => s.entered);
  return entered ? <App /> : <Login />;
}

const qc = new QueryClient({
  defaultOptions: { queries: { staleTime: 3000, refetchOnWindowFocus: false, retry: 1 } },
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={qc}>
      <BrowserRouter>
        <Root />
        <Toaster />
        <ConfirmHost />
      </BrowserRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
