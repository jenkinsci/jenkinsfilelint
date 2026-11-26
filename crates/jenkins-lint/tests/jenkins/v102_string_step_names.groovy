pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                'echo' 'hi'
                'echo' message: 'named'
                'retry'(2) {
                    echo 'again'
                }
            }
        }
    }
}
