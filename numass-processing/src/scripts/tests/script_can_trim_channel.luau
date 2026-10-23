return function(frame: Frame)
    for _, channel in frame do
        channel.samples[#channel.samples] = nil
    end
end
