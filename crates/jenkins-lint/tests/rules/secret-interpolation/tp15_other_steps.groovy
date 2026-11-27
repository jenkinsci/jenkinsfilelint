pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    echo "token: ${TOKEN}"
                    writeFile file: 'token.txt', text: "${TOKEN}"
                }
            }
        }
    }
}
