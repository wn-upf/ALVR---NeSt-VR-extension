#pragma once

#include "ALVR-common/packet_types.h"
#include "TrackedDevice.h"
#include "openvr_driver.h"
#include <memory>
#ifdef _WIN32
#include "platform/win32/OvrDirectModeComponent.h"
#endif

class ClientConnection;
class VSyncThread;

class OvrController;
class OvrController;
class OvrViveTrackerProxy;

class CEncoder;
#ifdef _WIN32
class CD3DRender;
#endif
class PoseHistory;

class OvrHmd : public TrackedDevice,
               public vr::ITrackedDeviceServerDriver,
               vr::IVRDisplayComponent {
  public:
    OvrHmd();

    virtual ~OvrHmd();

    std::string GetSerialNumber() const;

    virtual vr::EVRInitError Activate(vr::TrackedDeviceIndex_t unObjectId);

    virtual void Deactivate();
    virtual void EnterStandby();

    void *GetComponent(const char *pchComponentNameAndVersion);

    /** debug request from a client */
    virtual void
    DebugRequest(const char *pchRequest, char *pchResponseBuffer, uint32_t unResponseBufferSize);

    virtual vr::DriverPose_t GetPose();

    void RunFrame();

    void OnPoseUpdated();

    void StartStreaming();

    void StopStreaming();

    void OnStreamStart();

    void OnPacketLoss();

    void OnShutdown();

    void RequestIDR();

    void updateController(const TrackingInfo &info);

    void SetViewsConfig(ViewsConfigData config);

    bool IsTrackingRef() const { return m_deviceClass == vr::TrackedDeviceClass_TrackingReference; }
    bool IsHMD() const { return m_deviceClass == vr::TrackedDeviceClass_HMD; }

    // IVRDisplayComponent

    virtual void GetWindowBounds(int32_t *pnX, int32_t *pnY, uint32_t *pnWidth, uint32_t *pnHeight);

    virtual bool IsDisplayOnDesktop();

    virtual bool IsDisplayRealDisplay();

    virtual void GetRecommendedRenderTargetSize(uint32_t *pnWidth, uint32_t *pnHeight);

    virtual void GetEyeOutputViewport(
        vr::EVREye eEye, uint32_t *pnX, uint32_t *pnY, uint32_t *pnWidth, uint32_t *pnHeight);

    virtual void
    GetProjectionRaw(vr::EVREye eEye, float *pfLeft, float *pfRight, float *pfTop, float *pfBottom);

    virtual vr::DistortionCoordinates_t ComputeDistortion(vr::EVREye eEye, float fU, float fV);

    std::shared_ptr<ClientConnection> m_Listener;
    float m_poseTimeOffset;

    vr::VRInputComponentHandle_t m_proximity;

    std::shared_ptr<OvrController> m_leftController;
    std::shared_ptr<OvrController> m_rightController;

  private:
    ViewsConfigData views_config;

    bool m_baseComponentsInitialized;
    bool m_streamComponentsInitialized;
    vr::ETrackedDeviceClass m_deviceClass;

    vr::HmdMatrix34_t m_eyeToHeadLeft;
    vr::HmdMatrix34_t m_eyeToHeadRight;
    vr::HmdRect2_t m_eyeFoVLeft;
    vr::HmdRect2_t m_eyeFoVRight;

    std::wstring m_adapterName;

#ifdef _WIN32
    std::shared_ptr<CD3DRender> m_D3DRender;
#endif
    std::shared_ptr<CEncoder> m_encoder;
    std::shared_ptr<VSyncThread> m_VSyncThread;

#ifdef _WIN32
    std::shared_ptr<OvrDirectModeComponent> m_directModeComponent;
#endif
    std::shared_ptr<PoseHistory> m_poseHistory;

    std::shared_ptr<OvrViveTrackerProxy> m_viveTrackerProxy;
};
