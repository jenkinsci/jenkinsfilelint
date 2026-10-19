// Top comment
/* block
   comment */
pipeline { // trailing
    agent any /* inline */
    stages {
        stage('构建 Build 🚀') {
            steps {
                echo '你好，世界'  // Chinese
                sh '''
                    echo "多行 ${HOME}"
                '''
            }
        }
    }
}
