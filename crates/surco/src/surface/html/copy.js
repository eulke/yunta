// A command's button copies it. Where the browser refuses, the command is selected, and the reader copies it.
document.querySelectorAll('.cmd').forEach(function (button) {
  var said = button.querySelector('.act');
  var word = said.textContent;
  button.addEventListener('click', function () {
    var copied = function () {
      button.classList.add('copied');
      said.textContent = 'copied';
      setTimeout(function () {
        button.classList.remove('copied');
        said.textContent = word;
      }, 1600);
    };
    var selected = function () {
      var range = document.createRange();
      range.selectNodeContents(button.querySelector('.line'));
      var selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(button.getAttribute('data-copy')).then(copied, selected);
    } else {
      selected();
    }
  });
});
