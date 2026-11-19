pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                dir 'sub' {
                    echo 'x'
                }
            }
        }
    }
}
