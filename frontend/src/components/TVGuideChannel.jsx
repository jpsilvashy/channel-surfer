import { useState, useEffect } from 'react'

const TVGuideChannel = () => {
  const [currentTime, setCurrentTime] = useState(new Date())
  const [selectedChannel, setSelectedChannel] = useState(null)

  const channels = [
    {
      id: 1,
      number: "3",
      network: "WKYC",
      video: "/videos/WKYC 6pm News, June 2001.ia.mp4",
      schedule: [
        { time: "6:00 PM", show: "Evening News", duration: 30 },
        { time: "6:30 PM", show: "NBC Nightly News", duration: 30 },
        { time: "7:00 PM", show: "Entertainment Tonight", duration: 30 },
        { time: "7:30 PM", show: "Family Ties", duration: 30 }
      ]
    },
    {
      id: 2,
      number: "4",
      network: "WUAB",
      video: "/videos/CaptureA_3483.mp4",
      schedule: [
        { time: "6:00 PM", show: "Movie: Back to the Future", duration: 150 },
        { time: "8:30 PM", show: "Local Programming", duration: 30 }
      ]
    },
    {
      id: 3,
      number: "5",
      network: "WEWS",
      video: "/videos/sample.mp4",
      schedule: [
        { time: "6:00 PM", show: "ABC World News", duration: 30 },
        { time: "6:30 PM", show: "Wheel of Fortune", duration: 30 },
        { time: "7:00 PM", show: "Jeopardy!", duration: 30 },
        { time: "7:30 PM", show: "Inside Edition", duration: 30 }
      ]
    },
    {
      id: 4,
      number: "8",
      network: "WJW",
      video: "/videos/sample.mp4",
      schedule: [
        { time: "6:00 PM", show: "FOX 8 News", duration: 60 },
        { time: "7:00 PM", show: "The Simpsons", duration: 30 },
        { time: "7:30 PM", show: "Married... with Children", duration: 30 }
      ]
    },
    {
      id: 5,
      number: "19",
      network: "WOIO",
      video: "/videos/sample.mp4",
      schedule: [
        { time: "6:00 PM", show: "CBS Evening News", duration: 30 },
        { time: "6:30 PM", show: "Access Hollywood", duration: 30 },
        { time: "7:00 PM", show: "Movie: Raiders of the Lost Ark", duration: 120 }
      ]
    },
    {
      id: 6,
      number: "25",
      network: "WVIZ",
      video: "/videos/sample.mp4",
      schedule: [
        { time: "6:00 PM", show: "PBS NewsHour", duration: 60 },
        { time: "7:00 PM", show: "Nature: Arctic Wildlife", duration: 90 },
        { time: "8:30 PM", show: "Nova", duration: 30 }
      ]
    }
  ]

  useEffect(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date())
    }, 1000)
    return () => clearInterval(timer)
  }, [])

  const formatTime = (date) => {
    return date.toLocaleTimeString('en-US', {
      hour: 'numeric',
      minute: '2-digit',
      hour12: true
    })
  }

  const getWidthFromDuration = (duration) => {
    // Base width is 180px for a 30-minute show
    return (duration / 30) * 180 + 'px'
  }

  if (selectedChannel) {
    return (
      <div className="video-player">
        <video
          src={selectedChannel.video}
          autoPlay
          controls
          className="full-video"
        />
        <button
          onClick={() => setSelectedChannel(null)}
          className="back-button"
        >
          Back to Guide
        </button>
      </div>
    )
  }

  return (
    <div className="tv-guide">
      <div className="guide-header">
        <h1>TV GUIDE</h1>
        <div className="current-time">{formatTime(currentTime)}</div>
      </div>

      <div className="channel-grid">
        {channels.map((channel) => (
          <div key={channel.id} className="channel-row">
            <div className="channel-info">
              <div className="channel-content">
                <div className="channel-number">CH {channel.number}</div>
                <div className="network-name">{channel.network}</div>

              </div>
            </div>
            
            <div 
              className="program-list"
              onClick={() => setSelectedChannel(channel)}
            >
              {channel.schedule.map((program, idx) => (
                <div 
                  key={idx} 
                  className="program-item"
                  style={{ width: getWidthFromDuration(program.duration) }}
                >
                  <span className="program-time">{program.time}</span>
                  <span className="program-title">{program.show}</span>
                  <span className="program-duration">{program.duration} min</span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

export default TVGuideChannel
