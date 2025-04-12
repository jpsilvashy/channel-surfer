import { useState } from 'react'
import TVGuideChannel from './components/TVGuideChannel'

function App() {
  const [isLoaded, setIsLoaded] = useState(true)

  return (
    <div className="app-container">
      {isLoaded && <TVGuideChannel />}
    </div>
  )
}

export default App
