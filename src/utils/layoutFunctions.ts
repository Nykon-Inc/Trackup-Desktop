export const fadeInBackgroundElements = () => {
  setTimeout(() => {
    document.getElementById("design-element-1")!.style.opacity = "1";
    document.getElementById("design-element-1")!.style.transform =
      "translateY(0)";
  }, 100);

  setTimeout(() => {
    document.getElementById("design-element-2")!.style.opacity = "1";
    document.getElementById("design-element-2")!.style.transform =
      "translateY(0)";
  }, 300);

  setTimeout(() => {
    document.getElementById("design-element-3")!.style.opacity = "1";
    document.getElementById("design-element-3")!.style.transform =
      "translateX(0)";
  }, 500);
};
