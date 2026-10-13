import copy
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch
from demo_media import quality, check, verify, run


def sidecar(hz=60):
    return {"version":1,"failure":None,"size":[320,240],"duration":16,"gestures":[{"id":0,"start":0,"end":1000}],"inputs":[{"type":"mousemove","time":n*1000/hz,"x":n,"y":50,"inside":True,"gesture":0} for n in range(hz)]}


class CaptureQuality(unittest.TestCase):
    def test_true_source_and_cursor_cadence_are_separate(self):
        times=[n/30 for n in range(480)]
        result=quality(times,sidecar(),0,15)
        self.assertTrue(result["qualified"])
        self.assertAlmostEqual(result["source"]["decodedFps"],30)
        self.assertAlmostEqual(result["cursor"]["medianHz"],60)
        self.assertFalse(quality(times,sidecar(15),0,15)["qualified"])
        self.assertFalse(quality([n/15 for n in range(240)],sidecar(),0,15)["qualified"])

    def test_empty_occluded_stalled_and_regressed_source(self):
        with self.assertRaisesRegex(ValueError,"No usable"):
            quality([],sidecar(),0,15)
        with self.assertRaisesRegex(ValueError,"strictly increasing"):
            quality([0,.1,.1],sidecar(),0,15)
        times=[n/60 for n in range(900) if not 300<=n<340]
        self.assertFalse(quality(times,sidecar(),0,15)["qualified"])

    def test_stationary_holds_are_not_lag_but_unreceived_gestures_are(self):
        times=[n/30 for n in range(480)]
        meta=sidecar();meta["inputs"].extend({"type":"mousemove","time":t,"x":59,"y":50,"inside":True,"gesture":None} for t in (2000,5000,14000))
        self.assertTrue(quality(times,meta,0,15)["qualified"])
        meta["inputs"]=[]
        self.assertFalse(quality(times,meta,0,15)["qualified"])
        meta["gestures"]=[]
        self.assertTrue(quality(times,meta,0,15)["qualified"])

    def test_previous_received_cursor_can_be_held_at_the_recording_start(self):
        meta=sidecar();meta['inputs'].insert(0,{"type":"mousemove","time":-500,"x":5,"y":5,"inside":True,"gesture":None})
        self.assertTrue(quality([n/30 for n in range(480)],meta,0,15)['qualified'])
        with self.assertRaisesRegex(ValueError,'Invalid quality'):
            quality([0,.1],meta,float('nan'),1)
        meta['gestures'][0]['end']=float('inf')
        with self.assertRaisesRegex(ValueError,'Invalid gesture'):
            quality([0,.1],meta,0,1)

    def test_failures_and_incomplete_gestures_remain_failures(self):
        meta=sidecar();meta["failure"]="encoding error"
        with self.assertRaisesRegex(ValueError,"failed"):
            quality([0,.1],meta,0,1)
        meta=sidecar();meta["gestures"][0]["end"]=None
        with self.assertRaisesRegex(ValueError,"Incomplete"):
            quality([n/30 for n in range(60)],meta,0,1)

    def test_decode_real_tiny_media_and_reject_dimension_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            source=pathlib.Path(directory)/"source.webm";metadata=source.with_suffix('.json')
            run(["ffmpeg","-v","error","-f","lavfi","-i","color=black:s=320x240:r=30","-t","16","-an","-c:v","libvpx","-threads","1",str(source)])
            meta=sidecar();metadata.write_text(json.dumps(meta))
            self.assertTrue(check(source,metadata,0,15)["qualified"])
            meta["size"]=[1200,900];metadata.write_text(json.dumps(meta))
            with self.assertRaisesRegex(ValueError,"dimensions"):
                check(source,metadata,0,15)
            with self.assertRaisesRegex(ValueError,"outside"):
                metadata.write_text(json.dumps(sidecar()));check(source,metadata,10,15)

    def test_export_rejects_wrong_codec_dimensions_frames_audio_and_decode_errors(self):
        valid={"streams":[{"codec_name":"h264","pix_fmt":"yuv420p","width":1920,"height":1080,"r_frame_rate":"60/1","nb_read_frames":"900"}],"format":{"duration":"15.000"}}
        with tempfile.TemporaryDirectory() as directory:
            file=pathlib.Path(directory)/"demo.mp4"
            file.write_bytes(b'\0\0\0\x08moov\0\0\0\x08mdat')
            for key,value in (("codec_name","vp8"),("width",1280),("nb_read_frames","450")):
                invalid=copy.deepcopy(valid);invalid["streams"][0][key]=value
                with patch('demo_media.probe',return_value=invalid), self.assertRaisesRegex(ValueError,"Invalid export"):
                    verify(file)
            invalid=copy.deepcopy(valid);invalid["streams"].append({"codec_type":"audio"})
            with patch('demo_media.probe',return_value=invalid), self.assertRaisesRegex(ValueError,"no audio"):
                verify(file)
            with patch('demo_media.probe',return_value=valid), patch('demo_media.run',side_effect=RuntimeError('decode failure')), self.assertRaisesRegex(RuntimeError,"decode failure"):
                verify(file)
            file.write_bytes(b'\0\0\0\x08mdat\0\0\0\x08moov')
            with patch('demo_media.probe',return_value=valid), self.assertRaisesRegex(ValueError,"faststart"):
                verify(file)


if __name__ == '__main__':
    unittest.main()
