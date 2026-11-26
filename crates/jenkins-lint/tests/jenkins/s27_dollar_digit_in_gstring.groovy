pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "awk '{print $1}' file.txt"
            }
        }
    }
}
