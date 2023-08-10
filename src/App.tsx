import {invoke} from "@tauri-apps/api/tauri";
import './style.css'
import './index.css'
import {useEffect, useState} from "react";


type Config = {
    automatic_switching: boolean
}

export const App = ()=>{
    const [sunsetRise, setSunsetRise] = useState<boolean>()


    useEffect(()=>{
        invoke('get_config')
            .then(c=>{
                const typed_c = c as Config
                setSunsetRise(typed_c.automatic_switching)
            })
    },[])
    const change_theme = async (thene: string) => {
        await invoke('change_theme_handler',{
            themeSelection: thene
        })
    }


    return   (<div className="container">
            <>
            <h1>Mac OS Theme switcher</h1>
            <button onClick={()=>change_theme('Light')}>Light mode</button>
            <button onClick={()=>change_theme('Dark')}>Dark mode</button>
                <div className="flex gap-5 justify-center">
                    <input type="checkbox" id="adapt-to-sun" className="self-center" checked={sunsetRise} onChange={()=>{
                        invoke('change_sunset_option', {
                            activated: !sunsetRise
                        })
                        setSunsetRise(!sunsetRise)
                    }}></input>
                    <label htmlFor="adapt-to-sun">Sunset/Sunrise</label>
                </div>
            </>
    </div>)
}