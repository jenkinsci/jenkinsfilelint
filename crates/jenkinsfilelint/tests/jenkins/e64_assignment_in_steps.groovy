pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                result = 'x'
            }
        }
    }
}
