import './style.css'
import ReactDOM from "react-dom/client";
import {App} from './App'
import React from "react";
import "./index.css"

ReactDOM.createRoot(document.getElementById("app") as HTMLElement).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>
);