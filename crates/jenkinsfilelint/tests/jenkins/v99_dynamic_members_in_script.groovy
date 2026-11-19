pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                script {
                    def name = 'TARGET'
                    env.(name) = 'staging'
                    echo env.(name)
                    for (envVar in ['A=1', 'B=2']) {
                        env.(envVar.split('=')[0]) = envVar.split('=')[1]
                    }
                    def handlers = [{ msg -> echo msg }]
                    handlers[0] 'hello'
                    def steps = [say: { msg -> echo msg }]
                    steps['say'] 'hi'
                    handlers[0] { echo 'in a closure' }
                    this.('echo') 'dynamic'
                }
            }
        }
    }
}
